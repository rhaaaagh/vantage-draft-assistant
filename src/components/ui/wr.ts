/** Класс цвета винрейта по порогам: высокий зелёный / средний жёлтый / низкий красный. */
export function wrClass(wr: number): string {
  if (wr >= 55) return 'ui-wr--high'
  if (wr >= 50) return 'ui-wr--mid'
  return 'ui-wr--low'
}
