import { useEffect, useMemo } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useAuthStore } from "../auth/store";
import { followedStreamsQueryKey } from "../notifications/followedLive";
import { getFollowedStreams, LIVE_STREAM_QUERY } from "../twitch/helix";

export function useFollowedLiveStreams() {
  const session = useAuthStore((s) => s.session);
  const loggedIn = Boolean(session?.loggedIn && session.userId);

  const query = useInfiniteQuery({
    queryKey: followedStreamsQueryKey(session?.userId),
    enabled: loggedIn,
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) =>
      getFollowedStreams(session!.userId!, pageParam),
    getNextPageParam: (last) => last.pagination?.cursor,
    ...LIVE_STREAM_QUERY,
  });

  useEffect(() => {
    if (query.hasNextPage && !query.isFetchingNextPage) {
      void query.fetchNextPage();
    }
  }, [query.hasNextPage, query.isFetchingNextPage, query.fetchNextPage]);

  const streams = useMemo(
    () => query.data?.pages.flatMap((page) => page.data) ?? [],
    [query.data],
  );

  return { query, streams, loggedIn };
}
