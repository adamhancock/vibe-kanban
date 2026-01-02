import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2, AlertCircle, FileDown, ExternalLink } from 'lucide-react';
import { notionImportApi } from '@/lib/api';
import type {
  NotionImportPreviewItem,
  NotionImportPreviewResponse,
  TaskStatus,
} from 'shared/types';

export interface NotionImportDialogProps {
  projectId: string;
}

const statusBadgeVariant = (
  status: TaskStatus
): 'default' | 'secondary' | 'outline' => {
  switch (status) {
    case 'inprogress':
      return 'default';
    case 'done':
      return 'secondary';
    default:
      return 'outline';
  }
};

const statusLabel = (status: TaskStatus): string => {
  switch (status) {
    case 'todo':
      return 'To Do';
    case 'inprogress':
      return 'In Progress';
    case 'inreview':
      return 'In Review';
    case 'done':
      return 'Done';
    case 'cancelled':
      return 'Cancelled';
    default:
      return status;
  }
};

const NotionImportDialogImpl = NiceModal.create<NotionImportDialogProps>(
  ({ projectId }) => {
    const modal = useModal();
    const queryClient = useQueryClient();
    const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

    // Fetch preview data
    const {
      data: preview,
      isLoading,
      error,
    } = useQuery<NotionImportPreviewResponse>({
      queryKey: ['notion-import-preview', projectId],
      queryFn: () => notionImportApi.preview(projectId),
      staleTime: 0,
    });

    const importableTasks = preview?.tasks.filter((t) => t.will_import) ?? [];
    const allSelected = importableTasks.length > 0 &&
      importableTasks.every((t) => selectedIds.has(t.notion_id));

    const handleSelectAll = () => {
      if (allSelected) {
        // Deselect all
        setSelectedIds(new Set());
      } else {
        // Select all importable
        setSelectedIds(new Set(importableTasks.map((t) => t.notion_id)));
      }
    };

    // Import mutation
    const importMutation = useMutation({
      mutationFn: (notionIds: string[]) =>
        notionImportApi.import(projectId, notionIds),
      onSuccess: () => {
        queryClient.invalidateQueries({ queryKey: ['tasks'] });
        modal.resolve();
        modal.hide();
      },
    });

    const toggleTask = (notionId: string) => {
      setSelectedIds((prev) => {
        const next = new Set(prev);
        if (next.has(notionId)) {
          next.delete(notionId);
        } else {
          next.add(notionId);
        }
        return next;
      });
    };

    const handleImport = async () => {
      await importMutation.mutateAsync(Array.from(selectedIds));
    };

    const handleClose = () => {
      modal.reject();
      modal.hide();
    };

    return (
      <Dialog open={modal.visible} onOpenChange={(open) => !open && handleClose()}>
        <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col">
          <DialogHeader>
            <div className="flex items-center justify-between">
              <DialogTitle className="flex items-center gap-2">
                <FileDown className="h-5 w-5" />
                Import from Notion
              </DialogTitle>
              {preview && importableTasks.length > 0 && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleSelectAll}
                >
                  {allSelected ? 'Deselect All' : 'Select All'}
                </Button>
              )}
            </div>
            <DialogDescription>
              Select tasks to import from your Notion workspace
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 overflow-y-auto py-4 min-h-0">
            {isLoading && (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-6 w-6 animate-spin" />
                <span className="ml-2">Loading tasks from Redis...</span>
              </div>
            )}

            {error && (
              <Alert variant="destructive">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>
                  {error instanceof Error
                    ? error.message
                    : 'Failed to load tasks'}
                </AlertDescription>
              </Alert>
            )}

            {preview && preview.tasks.length === 0 && (
              <div className="text-center py-8 text-muted-foreground">
                No tasks found in Redis. Make sure workstream-daemon is running
                and REDIS_URL is configured.
              </div>
            )}

            {preview && preview.tasks.length > 0 && (
              <div className="space-y-2">
                {preview.tasks.map((task) => (
                  <TaskPreviewItem
                    key={task.notion_id}
                    task={task}
                    isSelected={selectedIds.has(task.notion_id)}
                    onToggle={() => toggleTask(task.notion_id)}
                  />
                ))}
              </div>
            )}
          </div>

          {importMutation.error && (
            <Alert variant="destructive" className="mt-2">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>
                {importMutation.error instanceof Error
                  ? importMutation.error.message
                  : 'Failed to import tasks'}
              </AlertDescription>
            </Alert>
          )}

          <DialogFooter className="flex items-center justify-between sm:justify-between">
            {preview && (
              <div className="text-sm text-muted-foreground">
                {preview.importable_count} importable
                {preview.duplicate_count > 0 &&
                  `, ${preview.duplicate_count} duplicates`}
              </div>
            )}
            <div className="flex gap-2">
              <Button variant="outline" onClick={handleClose}>
                Cancel
              </Button>
              <Button
                onClick={handleImport}
                disabled={selectedIds.size === 0 || importMutation.isPending}
              >
                {importMutation.isPending ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Importing...
                  </>
                ) : (
                  `Import ${selectedIds.size} Task${selectedIds.size !== 1 ? 's' : ''}`
                )}
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

interface TaskPreviewItemProps {
  task: NotionImportPreviewItem;
  isSelected: boolean;
  onToggle: () => void;
}

function TaskPreviewItem({ task, isSelected, onToggle }: TaskPreviewItemProps) {
  return (
    <div
      className={`flex items-start gap-3 p-3 rounded-lg border ${
        !task.will_import ? 'opacity-60 bg-muted' : 'hover:bg-accent'
      }`}
    >
      <Checkbox
        checked={isSelected}
        onCheckedChange={onToggle}
        disabled={!task.will_import}
        className="mt-0.5"
      />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="font-medium truncate">{task.title}</span>
          <Badge variant={statusBadgeVariant(task.status)}>
            {statusLabel(task.status)}
          </Badge>
          {task.task_id && (
            <span className="text-xs text-muted-foreground">{task.task_id}</span>
          )}
        </div>
        {task.skip_reason && (
          <span className="text-xs text-amber-600 dark:text-amber-500">
            {task.skip_reason}
          </span>
        )}
        {task.description && (
          <p className="text-sm text-muted-foreground mt-1 line-clamp-2">
            {task.description}
          </p>
        )}
      </div>
      <a
        href={task.url}
        target="_blank"
        rel="noopener noreferrer"
        className="text-muted-foreground hover:text-foreground flex-shrink-0"
        onClick={(e) => e.stopPropagation()}
      >
        <ExternalLink className="h-4 w-4" />
      </a>
    </div>
  );
}

export const NotionImportDialog = defineModal<NotionImportDialogProps, void>(
  NotionImportDialogImpl
);
