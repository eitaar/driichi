import { useQueryClient, type QueryKey, type QueryObserverResult } from "@tanstack/react-query";
import { ApiProblem } from "./api";

// Recovery must not apply a snapshot whose read began before the mutation outcome.
export function usePostMutationRefresh<T>(queryKey: QueryKey, refetch: (options: { cancelRefetch: false }) => Promise<QueryObserverResult<T>>) {
  const client = useQueryClient();
  return async () => {
    const query = client.getQueryCache().find({ queryKey, exact: true });
    if (!query || query.getObserversCount() === 0) return undefined;
    if (query.state.fetchStatus === "fetching") {
      const previous = await refetch({ cancelRefetch: false });
      if (previous.error instanceof ApiProblem && previous.error.status === 401) return undefined;
    }
    if (client.getQueryCache().find({ queryKey, exact: true }) !== query || query.getObserversCount() === 0) return undefined;
    return refetch({ cancelRefetch: false });
  };
}
