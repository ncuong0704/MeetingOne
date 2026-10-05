/** Dispose a late registration too, if cleanup happens before its promise resolves. */
export function subscribeSafely(register: () => Promise<() => void>, onError: (error: unknown) => void): () => void {
  let disposed = false;
  let unsubscribe: (() => void) | undefined;
  void Promise.resolve().then(register).then((cleanup) => {
    if (disposed) cleanup();
    else unsubscribe = cleanup;
  }).catch((error) => { if (!disposed) onError(error); });
  return () => {
    if (disposed) return;
    disposed = true;
    unsubscribe?.();
  };
}
