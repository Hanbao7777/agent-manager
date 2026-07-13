import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  CheckCircle2,
  CircleAlert,
  CircleMinus,
  LoaderCircle,
  XCircle,
} from "lucide-react";
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
  InstallFailure,
  InstallPreparation,
  InstallTaskSnapshot,
  RepairAction,
  ToolId,
  ToolInstallStatus,
} from "@/lib/api/installer";

interface ToolInstallDialogProps {
  open: boolean;
  preparation: InstallPreparation | null;
  task: InstallTaskSnapshot | null;
  toolName: (tool: ToolId) => string;
  onConfirm: (actionIds: string[]) => void;
  onCancel: () => void;
  onClose?: () => void;
  onRetry?: () => void;
}

const actionLabel = (action: RepairAction, t: (key: string) => string) =>
  t(`settings.installer.action.${action.kind}`);

const toolStatusPresentation: Record<
  ToolInstallStatus,
  { icon: typeof CheckCircle2; className: string }
> = {
  succeeded: { icon: CheckCircle2, className: "text-green-600" },
  failed: { icon: XCircle, className: "text-red-600" },
  installed_not_runnable: { icon: CircleAlert, className: "text-yellow-600" },
  skipped: { icon: CircleMinus, className: "text-muted-foreground" },
};

function FailureDetails({
  failure,
  t,
}: {
  failure: InstallFailure;
  t: (key: string) => string;
}) {
  return (
    <div className="mt-1 text-xs text-muted-foreground">
      <p>{t(`settings.installer.failure.${failure.code}`)}</p>
      {failure.detail && (
        <>
          <p className="mt-1 font-medium">
            {t("settings.installer.diagnostics")}
          </p>
          <p>{failure.detail}</p>
        </>
      )}
    </div>
  );
}

export function ToolInstallDialog({
  open,
  preparation,
  task,
  toolName,
  onConfirm,
  onCancel,
  onClose,
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
  const canRetry =
    result?.failure?.retryable ||
    result?.tools.some((tool) => tool.failure?.retryable);
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
        if (!next) (onClose ?? onCancel)();
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
            <p className="text-sm font-medium">
              {t(`settings.installer.resultStatus.${result.status}`)}
            </p>
            {result.failure && (
              <div className="rounded border border-red-500/20 bg-red-500/5 p-3 text-sm">
                <div className="flex items-center gap-2 font-medium">
                  <XCircle className="h-4 w-4 text-red-600" />
                  {t("settings.installer.failed")}
                </div>
                <FailureDetails failure={result.failure} t={t} />
              </div>
            )}
            {result.tools.map((tool) => {
              const presentation = toolStatusPresentation[tool.status];
              const Icon = presentation.icon;

              return (
                <div key={tool.tool} className="rounded border p-3 text-sm">
                  <div className="flex items-center gap-2 font-medium">
                    <Icon className={`h-4 w-4 ${presentation.className}`} />
                    <span>{toolName(tool.tool)}</span>
                    <span className="text-muted-foreground">
                      {t(`settings.installer.status.${tool.status}`)}
                    </span>
                  </div>
                  {tool.failure && (
                    <FailureDetails failure={tool.failure} t={t} />
                  )}
                </div>
              );
            })}
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
              {canRetry && onRetry && (
                <Button variant="outline" onClick={onRetry}>
                  {t("settings.installer.retry")}
                </Button>
              )}
              <Button onClick={onClose ?? onCancel}>
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
