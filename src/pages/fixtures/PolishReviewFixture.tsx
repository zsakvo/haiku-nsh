import { resource, Text, View } from "nsh";
import { Core } from "haiku/native";
import { ScrapeReview } from "../ScrapeReview";
import { createAppearance } from "../../ui/theme";

/** Empty optional metadata is intentional; the UI still resolves a real installed provider name. */
export function PolishReviewFixture() {
  createAppearance("light");
  const ready = resource(async () => {
    const core = await Core.openIsolated();
    const snapshot = {
      running: false, completed: 1, total: 1,
      items: [{ track: { id: "review", title: "等待审阅的歌曲" }, status: "review", automatic: false,
        candidates: [{ providerId: "qq.scraper", id: "candidate", title: "匹配后的歌曲",
          artists: ["林间回声"], album: "潮汐与回声", year: "", score: 0.8 }] }],
    };
    const review = {
      scrape: { jobs: () => snapshot, onChange: () => () => {} },
      library: { track: () => ({ title: "等待审阅的歌曲", artists: [], extra: {} }) },
    } as unknown as Core;
    return { core, review };
  });
  return <View class="flex-1 bg-background">
    <If when={!!ready()}><ScrapeReview core={ready()!.review} open onClose={() => {}} /></If>
    <If when={!!ready.error()}><Text>{String(ready.error())}</Text></If>
  </View>;
}
