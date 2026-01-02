import { useQuery } from '@tanstack/react-query';
import { executionProcessesApi } from '@/lib/api';

/**
 * Hook to fetch the devctl2 subdomain URL for an execution process.
 * Returns null if no devctl2 routing is configured for the process.
 */
export function useDevctl2Url(executionProcessId?: string) {
  return useQuery({
    queryKey: ['devctl2Url', executionProcessId],
    queryFn: async () => {
      if (!executionProcessId) return null;
      return executionProcessesApi.getDevctl2Url(executionProcessId);
    },
    enabled: !!executionProcessId,
    staleTime: Infinity, // URL won't change during execution
    refetchOnWindowFocus: false,
  });
}
