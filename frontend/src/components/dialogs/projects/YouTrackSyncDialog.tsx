import { useMemo, useState } from 'react';
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
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { integrationsApi } from '@/lib/api';

export type YouTrackSyncDialogProps = {
  projectId: string;
};

type ResultState =
  | { status: 'idle' }
  | { status: 'running' }
  | {
      status: 'done';
      openIssuesTotal: number;
      created: number;
      skippedExisting: number;
      dryRun: boolean;
      createdTitles: string[];
    }
  | { status: 'error'; message: string };

const YouTrackSyncDialogImpl = NiceModal.create<YouTrackSyncDialogProps>(
  ({ projectId }) => {
    const modal = useModal();
    const [boardUrl, setBoardUrl] = useState('');
    const [token, setToken] = useState('');
    const [stateField, setStateField] = useState('State');
    const [openValue, setOpenValue] = useState('Open');
    const [dryRun, setDryRun] = useState(true);
    const [result, setResult] = useState<ResultState>({ status: 'idle' });

    const canSync = useMemo(() => {
      return boardUrl.trim().length > 0 && token.trim().length > 0;
    }, [boardUrl, token]);

    const handleSync = async () => {
      setResult({ status: 'running' });
      try {
        const res = await integrationsApi.syncYouTrackOpen({
          project_id: projectId,
          board_url: boardUrl.trim(),
          youtrack_token: token.trim(),
          state_field: stateField.trim() || 'State',
          open_value: openValue.trim() || 'Open',
          dry_run: dryRun,
        });
        setResult({
          status: 'done',
          openIssuesTotal: res.open_issues_total,
          created: res.created,
          skippedExisting: res.skipped_existing,
          dryRun: res.dry_run,
          createdTitles: res.created_titles,
        });
      } catch (e) {
        const message = e instanceof Error ? e.message : 'Sync failed';
        setResult({ status: 'error', message });
      }
    };

    const handleClose = () => {
      modal.remove();
    };

    return (
      <Dialog
        open={modal.visible}
        onOpenChange={(open) => (open ? modal.show() : modal.hide())}
      >
        <DialogContent className="sm:max-w-[520px]">
          <DialogHeader>
            <DialogTitle>Sync YouTrack (Open → To Do)</DialogTitle>
            <DialogDescription>
              Paste a YouTrack Agile board URL (with sprint) and a token. Only
              issues in the sprint with State=Open are created as To Do tasks.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="youtrack-board-url">Board URL</Label>
              <Input
                id="youtrack-board-url"
                placeholder="https://host/youtrack/agiles/65-52/66-155467?a=..."
                value={boardUrl}
                onChange={(e) => setBoardUrl(e.target.value)}
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="youtrack-token">YouTrack token</Label>
              <Input
                id="youtrack-token"
                type="password"
                placeholder="perm:..."
                value={token}
                onChange={(e) => setToken(e.target.value)}
              />
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-2">
                <Label htmlFor="youtrack-state-field">State field</Label>
                <Input
                  id="youtrack-state-field"
                  value={stateField}
                  onChange={(e) => setStateField(e.target.value)}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="youtrack-open-value">Open value</Label>
                <Input
                  id="youtrack-open-value"
                  value={openValue}
                  onChange={(e) => setOpenValue(e.target.value)}
                />
              </div>
            </div>

            <div className="flex items-center justify-between rounded border p-3">
              <div className="space-y-1">
                <div className="text-sm font-medium">Dry run</div>
                <div className="text-xs text-muted-foreground">
                  Show what would be created without writing tasks.
                </div>
              </div>
              <Switch checked={dryRun} onCheckedChange={setDryRun} />
            </div>

            {result.status === 'done' ? (
              <div className="rounded border p-3 text-sm space-y-1">
                <div>
                  Open issues: <span className="font-medium">{result.openIssuesTotal}</span>
                </div>
                <div>
                  {result.dryRun ? 'Would create' : 'Created'}:{' '}
                  <span className="font-medium">{result.created}</span>
                </div>
                <div>
                  Skipped existing:{' '}
                  <span className="font-medium">{result.skippedExisting}</span>
                </div>
                {result.createdTitles.length > 0 ? (
                  <div className="pt-2 text-xs text-muted-foreground">
                    Latest: {result.createdTitles.slice(0, 3).join(' · ')}
                  </div>
                ) : null}
              </div>
            ) : null}

            {result.status === 'error' ? (
              <div className="rounded border border-destructive p-3 text-sm text-destructive">
                {result.message}
              </div>
            ) : null}
          </div>

          <DialogFooter className="gap-2">
            <Button variant="outline" onClick={handleClose}>
              Close
            </Button>
            <Button
              onClick={handleSync}
              disabled={!canSync || result.status === 'running'}
            >
              {result.status === 'running' ? 'Syncing…' : 'Sync'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const YouTrackSyncDialog = defineModal<YouTrackSyncDialogProps, void>(
  YouTrackSyncDialogImpl
);

