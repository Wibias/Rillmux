import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { LoadMore } from "../../components/LoadMore";
import { LoadingGrid } from "../../components/LoadingGrid";
import { PageRefreshButton } from "../../components/PageRefreshButton";
import { StreamGrid } from "../../components/StreamGrid";
import { PageSubbar } from "../../components/AppShell";
import { useAuthStore } from "../../lib/auth/store";
import { useWatchingStore } from "../../lib/streaming/store";
import { LanguageFilter } from "../../components/LanguageFilter";
import { getTopStreams, getUsersByLogin, LIVE_STREAM_QUERY } from "../../lib/twitch/helix";
import { languagesQueryKey } from "../../lib/twitch/languages";
import { useSettingsStore } from "../../lib/settings/store";

export function StreamsPage() {
  const { t } = useTranslation(["routes", "common"]);
  const session = useAuthStore((s) => s.session);
  const authLoading = useAuthStore((s) => s.loading);
  const watchStream = useWatchingStore((s) => s.watchStream);
  const loggedIn = Boolean(session?.loggedIn);
  const streamLanguages = useSettingsStore(
    (s) => s.settings.streaming.streamLanguages,
  );
  const langKey = languagesQueryKey(streamLanguages);

  const query = useInfiniteQuery({
    queryKey: ["top-streams", langKey],
    enabled: loggedIn,
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) => getTopStreams(pageParam, streamLanguages),
    getNextPageParam: (last) => last.pagination?.cursor,
    ...LIVE_STREAM_QUERY,
  });

  const streams = query.data?.pages.flatMap((p) => p.data) ?? [];
  const refreshing = query.isFetching && !query.isFetchingNextPage;
  const userLogins = useMemo(
    () => [
      ...new Set(
        (query.data?.pages ?? []).flatMap((page) =>
          page.data.map((stream) => stream.user_login.toLowerCase()),
        ),
      ),
    ],
    [query.data],
  );
  const usersQuery = useQuery({
    queryKey: ["top-stream-users", userLogins],
    enabled: loggedIn && userLogins.length > 0,
    queryFn: async () => {
      const page = await getUsersByLogin(userLogins);
      const record: Record<string, (typeof page.data)[number]> = {};
      for (const user of page.data) {
        record[user.login.toLowerCase()] = user;
      }
      return record;
    },
  });

  return (
    <section className="page">
      <PageSubbar
        title={t("routes:streamsTitle")}
        lede={t("routes:streamsLede")}
        actions={
          loggedIn ? (
            <>
              <LanguageFilter />
              <PageRefreshButton
                refreshing={refreshing}
                onRefresh={() => void query.refetch()}
              />
            </>
          ) : undefined
        }
      />
      {!loggedIn && !authLoading ? (
        <p className="muted">{t("routes:followedLoginRequired")}</p>
      ) : null}
      {(authLoading || query.isLoading) && !streams.length ? (
        <LoadingGrid />
      ) : null}
      {query.isError ? (
        <p className="muted">{(query.error as Error).message}</p>
      ) : null}
      {streams.length ? (
        <>
          <StreamGrid
            streams={streams}
            onWatch={(stream) => {
              void watchStream(stream);
            }}
            usersByLogin={usersQuery.data ?? {}}
          />
          <LoadMore
            hasMore={Boolean(query.hasNextPage)}
            isFetching={query.isFetchingNextPage}
            onLoadMore={() => void query.fetchNextPage()}
          />
        </>
      ) : null}
    </section>
  );
}

