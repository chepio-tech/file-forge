// Core
import { useCallback, useEffect, useRef, useState } from "react";
// Services
import fileforgeApi, { isAppError } from "@/services/fileforgeApi";
// Utils
import { errorMessage } from "@/utils/errorMessage";

export type UpdateState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "current"; version: string }
  | { status: "checkFailed" }
  /** `message`: why the last install attempt failed; the update can be installed again. */
  | { status: "available"; version: string; message?: string }
  /** Installing would discard unsaved results; waiting for the user's choice. */
  | { status: "confirm"; version: string }
  | { status: "installing"; version: string };

export interface Updates {
  state: UpdateState;
  /** A check the user asked for: its result and failures are shown. */
  check: () => void;
  install: (discardUnsaved?: boolean) => void;
  /** Leaves the unsaved-results warning without installing. */
  keep: () => void;
}

/**
 * Update state of the app (ADR-0013). Checks once on mount without reporting "up to date" or failures, so an
 * offline start stays quiet; a check the user starts reports both.
 */
export function useUpdates(): Updates {
  const [state, setState] = useState<UpdateState>({ status: "idle" });
  /** Only the latest check may change the state: an earlier, slower one is ignored. */
  const latestCheck = useRef(0);

  const runCheck = useCallback(async (manual: boolean) => {
    const id = ++latestCheck.current;
    setState({ status: "checking" });
    try {
      const { currentVersion, availableVersion } = await fileforgeApi.checkForUpdate();
      if (id !== latestCheck.current) return;
      if (availableVersion !== null) setState({ status: "available", version: availableVersion });
      else setState(manual ? { status: "current", version: currentVersion } : { status: "idle" });
    } catch {
      if (id !== latestCheck.current) return;
      setState(manual ? { status: "checkFailed" } : { status: "idle" });
    }
  }, []);

  useEffect(() => {
    void runCheck(false);
  }, [runCheck]);

  const check = useCallback(() => void runCheck(true), [runCheck]);

  const install = useCallback(
    (discardUnsaved = false) => {
      if (state.status !== "available" && state.status !== "confirm") return;
      const { version } = state;
      setState({ status: "installing", version });
      // On success the app restarts and this call never settles.
      fileforgeApi.installUpdate(discardUnsaved).catch((error: unknown) => {
        if (isAppError(error) && error.code === "unsavedResults") setState({ status: "confirm", version });
        else setState({ status: "available", version, message: errorMessage(error) });
      });
    },
    [state],
  );

  const keep = useCallback(() => {
    setState((current) => (current.status === "confirm" ? { status: "available", version: current.version } : current));
  }, []);

  return { state, check, install, keep };
}
