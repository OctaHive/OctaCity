/** Converts stable API enum values into compact operator-facing labels. */
export function formatEnumLabel(value: string): string {
  const words = value.replaceAll('_', ' ');
  return `${words.charAt(0).toUpperCase()}${words.slice(1)}`;
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

/** Formats an exact byte count for compact diagnostic tables. */
export function formatBytes(value: number): string {
  if (value < 1_024) return `${value} B`;
  if (value < 1_048_576) return `${(value / 1_024).toFixed(1)} KiB`;
  if (value < 1_073_741_824) return `${(value / 1_048_576).toFixed(1)} MiB`;
  return `${(value / 1_073_741_824).toFixed(1)} GiB`;
}
