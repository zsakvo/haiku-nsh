import { signal, View } from "nsh";
import { EmptyState } from "../ui/EmptyState";
import { SearchField } from "../ui/SearchField";
import type { destinations } from "../shell/navigation";

/** Every destination remains mounted, preserving native scroll and input state. */
export function PlaceholderPage(props: { page: typeof destinations[number]; active: boolean }) {
  const query = signal("");
  return <View testID={`page-${props.page.id}`} class="flex-1 flex-col min-h-0"
    style={{ display: props.active ? "flex" : "none" }}>
    {props.page.id === "search" && <View class="px-(--content-inset) pt-4 max-w-xl">
      <SearchField label="搜索词" value={query()} onChange={(value) => query.set(value)} placeholder="保留一个搜索词" />
    </View>}
    <EmptyState title={props.page.title} description={props.page.description} />
  </View>;
}
