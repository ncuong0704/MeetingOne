/** Parse markdown table header from `item_format` into column titles. */
export function parseTableColumns(itemFormat: string | undefined): string[] {
  if (!itemFormat?.trim()) return [];

  const headerLine = itemFormat.trim().split('\n').find(line => line.includes('|'));
  if (!headerLine) return [];

  return headerLine
    .split('|')
    .map(cell => cell.trim())
    .filter(Boolean);
}

/** Build `item_format` markdown table header + separator from column titles. */
export function buildItemFormat(columns: string[]): string {
  const titles = columns.map(c => c.trim()).filter(Boolean);
  if (titles.length === 0) return '';

  const header = `| ${titles.join(' | ')} |`;
  const separator = `| ${titles.map(() => '---').join(' | ')} |`;
  return `${header}\n${separator}`;
}
