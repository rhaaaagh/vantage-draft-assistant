use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Лимиты dev-ключа Riot: 20 запросов в секунду и 100 запросов за 2 минуты.
/// Скользящие окна; acquire() блокирует поток до освобождения слота в обоих окнах.
///
/// Приоритеты: интерактивные запросы (профиль, текущая игра, скаут, драфт) идут с
/// High, фоновый краулер — с Low. Краулер уступает, пока есть хоть один ждущий
/// High-запрос, и не занимает зарезервированные слоты — чтобы интерактиву всегда
/// было место даже при работающем краулере.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Priority {
    High,
    Low,
}

const LIMIT_1S: usize = 20;
const LIMIT_2M: usize = 100;
const WINDOW_1S: Duration = Duration::from_secs(1);
const WINDOW_2M: Duration = Duration::from_secs(120);

/// Слоты, которые краулер (Low) не трогает — резерв под интерактивные запросы.
const RESERVE_1S: usize = 5;
const RESERVE_2M: usize = 25;

/// Как часто Low перепроверяет, можно ли уже стартовать, пока уступает High.
const YIELD_POLL: Duration = Duration::from_millis(50);

struct Windows {
    window_1s: VecDeque<Instant>,
    window_2m: VecDeque<Instant>,
    /// Сколько High-запросов сейчас ждут слот. Пока > 0 — Low уступает.
    high_waiting: usize,
}

impl Windows {
    fn prune(&mut self, now: Instant) {
        while let Some(&t) = self.window_1s.front() {
            if now.duration_since(t) >= WINDOW_1S {
                self.window_1s.pop_front();
            } else {
                break;
            }
        }
        while let Some(&t) = self.window_2m.front() {
            if now.duration_since(t) >= WINDOW_2M {
                self.window_2m.pop_front();
            } else {
                break;
            }
        }
    }

    /// Сколько ждать до свободного слота (None — слот свободен прямо сейчас).
    fn wait_needed(&self, now: Instant, priority: Priority) -> Option<Duration> {
        // Краулер уступает дорогу, пока есть ждущие интерактивные запросы.
        if priority == Priority::Low && self.high_waiting > 0 {
            return Some(YIELD_POLL);
        }

        // Краулеру доступны не все слоты — последние RESERVE оставляем интерактиву.
        let (lim_1s, lim_2m) = match priority {
            Priority::High => (LIMIT_1S, LIMIT_2M),
            Priority::Low => (LIMIT_1S - RESERVE_1S, LIMIT_2M - RESERVE_2M),
        };

        let mut wait: Option<Duration> = None;
        if self.window_1s.len() >= lim_1s {
            let until = *self.window_1s.front().unwrap() + WINDOW_1S;
            wait = Some(until.saturating_duration_since(now));
        }
        if self.window_2m.len() >= lim_2m {
            let until = *self.window_2m.front().unwrap() + WINDOW_2M;
            let w = until.saturating_duration_since(now);
            wait = Some(wait.map_or(w, |cur| cur.max(w)));
        }
        wait
    }

    fn register(&mut self, now: Instant) {
        self.window_1s.push_back(now);
        self.window_2m.push_back(now);
    }
}

pub struct RateLimiter {
    inner: Mutex<Windows>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Windows {
                window_1s: VecDeque::new(),
                window_2m: VecDeque::new(),
                high_waiting: 0,
            }),
        }
    }

    /// Блокирует поток до свободного слота и регистрирует запрос.
    /// ВАЖНО: sleep выполняется БЕЗ удержания мьютекса, иначе ждущий поток
    /// блокирует всех остальных (из-за этого «зависал» поиск профиля при краулере).
    pub fn acquire(&self, priority: Priority) {
        // High отмечается как ждущий — пока он не получит слот, Low уступает.
        if priority == Priority::High {
            self.inner.lock().unwrap().high_waiting += 1;
        }
        loop {
            let wait = {
                let mut w = self.inner.lock().unwrap();
                let now = Instant::now();
                w.prune(now);
                match w.wait_needed(now, priority) {
                    None => {
                        w.register(now);
                        if priority == Priority::High {
                            w.high_waiting -= 1;
                        }
                        return;
                    }
                    Some(d) => d,
                }
            };
            std::thread::sleep(wait + Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Windows {
        Windows {
            window_1s: VecDeque::new(),
            window_2m: VecDeque::new(),
            high_waiting: 0,
        }
    }

    #[test]
    fn allows_burst_under_limit() {
        let mut w = fresh();
        let now = Instant::now();
        for _ in 0..LIMIT_1S - 1 {
            assert!(w.wait_needed(now, Priority::High).is_none());
            w.register(now);
        }
        assert!(w.wait_needed(now, Priority::High).is_none());
    }

    #[test]
    fn blocks_over_1s_limit() {
        let mut w = fresh();
        let now = Instant::now();
        for _ in 0..LIMIT_1S {
            w.register(now);
        }
        let wait = w.wait_needed(now, Priority::High);
        assert!(wait.is_some());
        assert!(wait.unwrap() <= WINDOW_1S);
    }

    #[test]
    fn blocks_over_2m_limit() {
        let mut w = fresh();
        let now = Instant::now();
        for _ in 0..LIMIT_2M {
            // окно 1с не заполняем, важно только 2м-окно
            w.window_2m.push_back(now);
        }
        let wait = w.wait_needed(now, Priority::High);
        assert!(wait.is_some());
        assert!(wait.unwrap() <= WINDOW_2M);
    }

    #[test]
    fn prunes_expired() {
        let mut w = fresh();
        let old = Instant::now() - Duration::from_secs(121);
        for _ in 0..LIMIT_2M {
            w.window_2m.push_back(old);
        }
        w.prune(Instant::now());
        assert!(w.wait_needed(Instant::now(), Priority::High).is_none());
        assert!(w.window_2m.is_empty());
    }

    #[test]
    fn low_yields_to_waiting_high() {
        let mut w = fresh();
        let now = Instant::now();
        // Окна пустые, но есть ждущий High — Low должен уступить.
        w.high_waiting = 1;
        assert!(w.wait_needed(now, Priority::Low).is_some());
        // High при этом проходит свободно.
        assert!(w.wait_needed(now, Priority::High).is_none());
    }

    #[test]
    fn low_respects_reserve_high_does_not() {
        let mut w = fresh();
        let now = Instant::now();
        // Заполняем 1с-окно до границы доступного краулеру (резерв занят интерактивом).
        for _ in 0..(LIMIT_1S - RESERVE_1S) {
            w.window_1s.push_back(now);
        }
        // Low уже упёрся в свой лимит, а High ещё может использовать резерв.
        assert!(w.wait_needed(now, Priority::Low).is_some());
        assert!(w.wait_needed(now, Priority::High).is_none());
    }

    // [SAFE-TEST] Когда заполнены ОБА окна, wait_needed возвращает МАКСИМУМ из
    // двух ожиданий (узкое место — более длинное 2м-окно), а не сумму/минимум.
    #[test]
    fn wait_needed_returns_max_of_two_windows() {
        let mut w = fresh();
        let now = Instant::now();
        // 1с-окно: самый старый слот был 200мс назад → ждать ~800мс (< 1с).
        let t1s = now - Duration::from_millis(200);
        for _ in 0..LIMIT_1S {
            w.window_1s.push_back(t1s);
        }
        // 2м-окно: самый старый слот был 30с назад → ждать ~90с.
        let t2m = now - Duration::from_secs(30);
        for _ in 0..LIMIT_2M {
            w.window_2m.push_back(t2m);
        }
        let wait = w.wait_needed(now, Priority::High).expect("оба окна полны");
        // Ожидание 1с-окна ≈ 800мс, 2м-окна ≈ 90с. Берём максимум → ~90с.
        let expected_2m = (t2m + WINDOW_2M).saturating_duration_since(now);
        let expected_1s = (t1s + WINDOW_1S).saturating_duration_since(now);
        assert!(expected_2m > expected_1s, "проверка предпосылки теста");
        assert_eq!(wait, expected_2m);
    }

    // [SAFE-TEST] Резерв в 2м-окне: при заполнении до (LIMIT_2M - RESERVE_2M)
    // Low уже заблокирован (его порог), а High ещё проходит (резерв его).
    #[test]
    fn low_blocked_at_2m_reserve_while_high_passes() {
        let mut w = fresh();
        let now = Instant::now();
        // Заполняем ТОЛЬКО 2м-окно до доступной краулеру границы.
        for _ in 0..(LIMIT_2M - RESERVE_2M) {
            w.window_2m.push_back(now);
        }
        // Low упёрся в свой 2м-лимит (95), High ещё имеет резерв до 100.
        assert!(w.wait_needed(now, Priority::Low).is_some());
        assert!(w.wait_needed(now, Priority::High).is_none());
    }

    // [SAFE-TEST] Граница prune: слот ровно WINDOW старый удаляется (условие
    // duration_since >= WINDOW), а слот чуть моложе WINDOW остаётся.
    #[test]
    fn prune_boundary_evicts_exactly_window_old() {
        let now = Instant::now();
        // Ровно WINDOW_1S назад → должен быть удалён (>=).
        let mut w = fresh();
        w.window_1s.push_back(now - WINDOW_1S);
        w.prune(now);
        assert!(w.window_1s.is_empty(), "слот ровно WINDOW старый должен удаляться");

        // Чуть моложе WINDOW_1S → остаётся.
        let mut w2 = fresh();
        w2.window_1s.push_back(now - WINDOW_1S + Duration::from_millis(1));
        w2.prune(now);
        assert_eq!(w2.window_1s.len(), 1, "слот моложе WINDOW должен остаться");

        // То же для 2м-окна на границе.
        let mut w3 = fresh();
        w3.window_2m.push_back(now - WINDOW_2M);
        w3.prune(now);
        assert!(w3.window_2m.is_empty());
    }
}
