import { cleanup, context, signal } from "nsh";

export type SourceOption = { id: string; title: string; logo?: string };
type PageSource = {
  active: () => boolean;
  items: () => SourceOption[];
  value: () => string;
  select: (value: string) => void;
};

/** The active page owns its source choice; the toolbar presents that same choice. */
export function createPageSource() {
  const pages = signal<PageSource[]>([]);
  return {
    current: () => pages().find((page) => page.active()),
    register(page: PageSource) {
      pages.update((value) => [...value, page]);
      return () => pages.update((value) => value.filter((entry) => entry !== page));
    },
  };
}

export const PageSourceContext = context<ReturnType<typeof createPageSource> | undefined>(undefined);

export function usePageSource(page: PageSource) {
  const toolbar = PageSourceContext();
  if (toolbar) cleanup(toolbar.register(page));
}
