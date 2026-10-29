/** Native reset-driven suggestion. The application owns the editable draft;
 * this browser-safe kernel also serves complete native lifetime comparisons. */
export function suggestProfileName(names) {
  const occupied = new Set(names);
  let number = 1;
  while (occupied.has(`Settings ${number}`)) number++;
  return `Settings ${number}`;
}
