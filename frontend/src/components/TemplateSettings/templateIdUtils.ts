export function slugifyTemplateName(name: string): string {
  return name
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/đ/gi, 'd')
    .toLowerCase()
    .replace(/[^a-z0-9\s_-]/g, '')
    .trim()
    .replace(/\s+/g, '_')
    .replace(/_+/g, '_')
    .slice(0, 40);
}

export function generateUniqueTemplateId(
  name: string,
  existingIds: Iterable<string>,
): string {
  const base = slugifyTemplateName(name);
  if (!base) return '';

  const ids = new Set(existingIds);
  let id = base;
  let counter = 2;
  while (ids.has(id)) {
    id = `${base}_${counter}`;
    counter += 1;
  }
  return id;
}
