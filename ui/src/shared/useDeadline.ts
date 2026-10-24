import { useEffect, useState } from 'react';

/** Retains the original deadline across timer clamping; callers key replaced receipts separately. */
export function useDeadline(expiresAt: number | null): boolean {
  const [expired, setExpired] = useState(() => expiresAt !== null && Date.now() >= expiresAt);
  useEffect(() => {
    if (expired || expiresAt === null) return;
    let timer: ReturnType<typeof setTimeout>;
    function schedule() {
      const remaining = expiresAt! - Date.now();
      if (remaining <= 0) setExpired(true);
      else timer = setTimeout(schedule, Math.min(remaining, 2_147_483_647));
    }
    timer = setTimeout(schedule, Math.min(Math.max(0, expiresAt - Date.now()), 2_147_483_647));
    return () => clearTimeout(timer);
  }, [expiresAt, expired]);
  return expired;
}
