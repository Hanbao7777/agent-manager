import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { CheckCircle2, LoaderCircle, XCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type {
  InstallPreparation,
  InstallTaskSnapshot,
  RepairAction,
  ToolId,
} from "@/lib/api/installer";

interface ToolInstallDialogProps {
  open: boolean;
  preparation: InstallPreparation | null;
  task: InstallTaskSnapshot | null;
  toolName: (tool: ToolId) => string;
  onConfirm: (actionIds: string[]) => void;
  onCancel: () => void;
  onRetry?: () => void;
}

const actionLabel = (action: RepairAction, t: (key: string) => string) =>
  t(`settings.installer.action.${action.kind}`);

export function ToolInstallDialog({
  open,
  preparation,
  task,
  toolName,
  onConfirm,
  onCancel,
  onRetry,
}: ToolInstallDialogProps) {
  const { t } = useTranslation();
  const [approved, setApproved] = useState<string[]>([]);
  const actions = preparation?.plan.actions ?? task?.plan.actions ?? [];
  const required = actions.filter(
    (action) => action.requires_confirmation || action.requires_elevation,
  );
  const result = task?.result;
  const isProgress = Boolean(task && !result);
  const canContinue =
    !preparation?.requires_confirmation ||
    required.every((action) => approved.includes(action.id));

  useEffect(() => {
    setApproved([]);
  }, [open, preparation?.task_id, task?.task_id]);

  const toggle = (id: string) =>
    setApproved((current) =>
      current.includes(id)
        ? current.filter((currentId) => currentId !== id)
        : [...current, id],
    );

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel();
      }}
    >
      <DialogContent className="max-w-md" zIndex="alert">
        <DialogHeader className="space-y-2 border-b-0 bg-transparent pb-0">
          <DialogTitle>
            {result
              ? t("settings.installer.result")
              : isProgress
                ? t("settings.installer.progress")
                : t("settings.installer.title")}
          </DialogTitle>
          <DialogDescription>
            {result
              ? t("settings.installer.resultHint")
              : isProgress
                ? t("settings.installer.progressHint")
                : t("settings.installer.preflightHint")}
          </DialogDescription>
        </DialogHeader>
        {result ? (
          <div className="space-y-2">
            {result.failure?.detail && (
              <div className="rounded border border-red-500/20 bg-red-500/5 p-3 text-sm">
                <div className="flex items-center gap-2 font-medium">
                  <XCircle className="h-4 w-4 text-red-600" />
                  {t("settings.installer.failed")}
                  <code className="text-xs text-muted-foreground">
                    {result.failure.code}
                  </code>
                </div>
                <p className="mt-1 text-xs font-medium text-muted-foreground">
                  {t("settings.installer.diagnostics")}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {result.failure.detail}
                </p>
              </div>
            )}
            {result.tools.map((tool) => (
              <div key={tool.tool} className="rounded border p-3 text-sm">
                <div className="flex items-center gap-2 font-medium">
                  {tool.status === "succeeded" ? (
                    <CheckCircle2 className="h-4 w-4 text-green-600" />
                  ) : (
                    <XCircle className="h-4 w-4 text-red-600" />
                  )}
                  <span>{toolName(tool.tool)}</span>
                  <span className="text-muted-foreground">
                    {tool.status === "succeeded"
                      ? t("settings.installer.installed")
                      : t("settings.installer.failed")}
                  </span>
                </div>
                {tool.failure?.detail && (
                  <div className="mt-1 text-xs text-muted-foreground">
                    <code>{tool.failure.code}</code>
                    <p>{tool.failure.detail}</p>
                  </div>
                )}
              </div>
            ))}
          </div>
        ) : isProgress ? (
          <div className="flex items-center gap-2 rounded border p-3 text-sm">
            <LoaderCircle className="h-4 w-4 animate-spin" />
            {t(`settings.installer.stage.${task!.stage}`)}
          </div>
        ) : (
          <div className="space-y-2">
            {actions.map((action) => (
              <label
                key={action.id}
                className="flex gap-2 rounded border p-3 text-sm"
              >
                {action.requires_confirmation || action.requires_elevation ? (
                  <input
                    type="checkbox"
                    checked={approved.includes(action.id)}
                    onChange={() => toggle(action.id)}
                  />
                ) : (
                  <CheckCircle2 className="h-4 w-4 text-green-600" />
                )}
                <span>
                  {actionLabel(action, t)}
                  {action.requires_elevation && (
                    <span className="block text-xs text-yellow-700 dark:text-yellow-400">
                      {t("settings.installer.administrator")}
                    </span>
                  )}
                </span>
              </label>
            ))}
          </div>
        )}
        <DialogFooter className="border-t-0 bg-transparent pt-2 sm:justify-end">
          {isProgress ? (
            <Button variant="outline" onClick={onCancel}>
              {t("settings.installer.cancel")}
            </Button>
          ) : result ? (
            <>
              {result.failure?.retryable && onRetry && (
                <Button variant="outline" onClick={onRetry}>
                  {t("settings.installer.retry")}
                </Button>
              )}
              <Button onClick={onCancel}>
                {t("settings.installer.close")}
              </Button>
            </>
          ) : (
            <>
              <Button variant="outline" onClick={onCancel}>
                {t("common.cancel")}
              </Button>
              <Button
                disabled={!canContinue}
                onClick={() => onConfirm(required.map((action) => action.id))}
              >
                {t("settings.installer.continue")}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
