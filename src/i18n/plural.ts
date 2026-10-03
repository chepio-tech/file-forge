const russianRules = new Intl.PluralRules("ru");

/** Picks the Russian plural form for `count` (1 файл, 2 файла, 5 файлов, 1,5 файла). */
export function plural(count: number, forms: { one: string; few: string; many: string }): string {
  const rule = russianRules.select(count);
  if (rule === "one") return forms.one;
  if (rule === "few" || rule === "other") return forms.few;
  return forms.many;
}

export default plural;
