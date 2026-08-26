import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useAuthStore } from "../lib/auth/store";
import {
  getFollowedStreams,
  getTopGames,
  getTopStreams,
} from "../lib/twitch/helix";
import { followedStreamsQueryKey } from "../lib/notifications/followedLive";

/** Keeps session restore and browse prefetching available without loading route UI. */
export function AuthBootstrap({ children }: { children: React.ReactNode }) {
  const refreshSession = useAuthStore((s) => s.refreshSession);
  const session = useAuthStore((s) => s.session);
  const queryClient = useQueryClient();

  useEffect(() => {
    void refreshSession();
  }, [refreshSession]);

  useEffect(() => {
    if (!session?.loggedIn) return;

    void queryClient.prefetchInfiniteQuery({
      queryKey: ["top-streams"],
      initialPageParam: undefined as string | undefined,
      queryFn: ({ pageParam }) => getTopStreams(pageParam),
      getNextPageParam: (last) => last.pagination?.cursor,
      staleTime: 20_000,
      pages: 1,
    });
    void queryClient.prefetchInfiniteQuery({
      queryKey: ["top-games"],
      initialPageParam: undefined as string | undefined,
      queryFn: ({ pageParam }) => getTopGames(pageParam),
      getNextPageParam: (last) => last.pagination?.cursor,
      staleTime: 60_000,
      pages: 1,
    });
    if (session.userId) {
      void queryClient.prefetchInfiniteQuery({
        queryKey: followedStreamsQueryKey(session.userId),
        initialPageParam: undefined as string | undefined,
        queryFn: ({ pageParam }) =>
          getFollowedStreams(session.userId!, pageParam),
        getNextPageParam: (last) => last.pagination?.cursor,
        staleTime: 20_000,
        pages: 1,
      });
    }
  }, [session?.loggedIn, session?.userId, queryClient]);

  return children;
}
