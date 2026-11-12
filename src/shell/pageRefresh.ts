import { cleanup, context, signal } from "nsh";

type PageRefreshAction = { run: () => void; disabled: () => boolean };
type RetainedPageRefresh = PageRefreshAction & { active: () => boolean };

/** Pages retain their own state and expose only their refresh command to the shell. */
export function createPageRefresh() {
  const actions = signal<RetainedPageRefresh[]>([]);
  return {
    current: () => actions().find((action) => action.active()),
    register(action: RetainedPageRefresh) {
      actions.update((value) => [...value, action]);
      return () => actions.update((value) => value.filter((entry) => entry !== action));
    },
  };
}

export const PageRefreshContext = context<ReturnType<typeof createPageRefresh> | undefined>(undefined);

export function usePageRefresh(active: () => boolean, action: PageRefreshAction) {
  const refresh = PageRefreshContext();
  if (refresh) cleanup(refresh.register({ ...action, active }));
}
