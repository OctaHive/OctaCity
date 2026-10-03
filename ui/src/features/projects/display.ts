/** Converts stable API enum values into compact operator-facing labels. */
export function formatEnumLabel(value: string): string {
  return `${value.charAt(0).toUpperCase()}${value.slice(1)}`;
}

/** Formats an epoch timestamp without emitting an invalid HTML dateTime value. */
export function formatTimestamp(value: number): { display: string; machine: string | null } {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return { display: 'Unknown', machine: null };
  }
  const machine = date.toISOString();
  return { display: `${machine.slice(0, -1).replace('T', ' ')} UTC`, machine };
}
