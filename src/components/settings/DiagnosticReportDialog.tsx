import { useState } from "react";
import { FileDown, Loader2, MessageSquareWarning, Send } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  diagnosticsApi,
  type DiagnosticReportPreview,
  type DiagnosticToolInput,
} from "@/lib/api";

interface DiagnosticReportDialogProps {
  tools: DiagnosticToolInput[];
}

export function DiagnosticReportDialog({
  tools,
}: DiagnosticReportDialogProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [summary, setSummary] = useState("");
  const [sensitive, setSensitive] = useState(false);
  const [preview, setPreview] = useState<DiagnosticReportPreview | null>(null);
  const [exportPath, setExportPath] = useState("");
  const [generating, setGenerating] = useState(false);
  const [exporting, setExporting] = useState(false);

  const reset = () => {
    setSummary("");
    setSensitive(false);
    setPreview(null);
    setExportPath("");
    setGenerating(false);
    setExporting(false);
  };

  const handleOpenChange = (nextOpen: boolean) => {
    setOpen(nextOpen);
    if (!nextOpen) reset();
  };

  const generatePreview = async () => {
    setGenerating(true);
    try {
      setPreview(await diagnosticsApi.generate({ summary, sensitive, tools }));
    } catch {
      toast.error(t("settings.diagnostics.generateFailed"));
    } finally {
      setGenerating(false);
    }
  };

  const exportReport = async () => {
    if (!preview || !exportPath.trim()) return;
    setExporting(true);
    try {
      await diagnosticsApi.export(preview.report_id, exportPath.trim());
      toast.success(t("settings.diagnostics.exported"));
    } catch {
      toast.error(t("settings.diagnostics.exportFailed"));
    } finally {
      setExporting(false);
    }
  };

  const openIssue = () => {
    if (!preview?.issue_url) return;
    window.open(preview.issue_url, "_blank", "noopener,noreferrer");
  };

  return (
    <>
      <Button
        size="sm"
        variant="outline"
        className="h-7 gap-1.5 text-xs"
        onClick={() => setOpen(true)}
      >
        <MessageSquareWarning className="h-3.5 w-3.5" />
        {t("settings.diagnostics.create")}
      </Button>
      <Dialog open={open} onOpenChange={handleOpenChange}>
        <DialogContent className="max-w-3xl">
          <DialogHeader>
            <DialogTitle>{t("settings.diagnostics.title")}</DialogTitle>
            <DialogDescription>
              {t("settings.diagnostics.description")}
            </DialogDescription>
          </DialogHeader>

          <div className="min-h-0 space-y-4 overflow-y-auto px-6 py-5">
            {!preview ? (
              <>
                <label className="block space-y-2 text-sm">
                  <span className="font-medium">
                    {t("settings.diagnostics.summary")}
                  </span>
                  <textarea
                    value={summary}
                    onChange={(event) => setSummary(event.target.value)}
                    rows={6}
                    className="w-full resize-y rounded-md border border-border bg-background px-3 py-2 font-mono text-xs"
                    placeholder={t("settings.diagnostics.summaryPlaceholder")}
                  />
                </label>
                <label className="flex items-start gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={sensitive}
                    onChange={(event) => setSensitive(event.target.checked)}
                    className="mt-1"
                  />
                  <span>{t("settings.diagnostics.sensitive")}</span>
                </label>
                <p className="text-xs text-muted-foreground">
                  {t("settings.diagnostics.collectedFields")}
                </p>
              </>
            ) : (
              <>
                <div className="space-y-2">
                  <label className="text-sm font-medium" htmlFor="issue-title">
                    {t("settings.diagnostics.issueTitle")}
                  </label>
                  <input
                    id="issue-title"
                    readOnly
                    value={preview.issue_title}
                    className="w-full rounded-md border border-border bg-muted/40 px-3 py-2 text-sm"
                  />
                </div>
                <div className="space-y-2">
                  <div className="text-sm font-medium">
                    {t("settings.diagnostics.issueBody")}
                  </div>
                  <pre
                    data-testid="diagnostic-issue-body"
                    className="max-h-72 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-muted/40 p-3 text-xs"
                  >
                    {preview.issue_body}
                  </pre>
                </div>
                {preview.public_block_reason && (
                  <p className="rounded-md border border-yellow-500/30 bg-yellow-500/10 p-3 text-xs text-yellow-700 dark:text-yellow-300">
                    {t(
                      `settings.diagnostics.blocked.${preview.public_block_reason}`,
                    )}
                  </p>
                )}
                <div className="space-y-2">
                  <label className="text-sm font-medium" htmlFor="export-path">
                    {t("settings.diagnostics.exportPath")}
                  </label>
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <input
                      id="export-path"
                      value={exportPath}
                      onChange={(event) => setExportPath(event.target.value)}
                      placeholder={t("settings.diagnostics.exportPlaceholder")}
                      className="min-w-0 flex-1 rounded-md border border-border bg-background px-3 py-2 font-mono text-xs"
                    />
                    <Button
                      variant="outline"
                      onClick={() => void exportReport()}
                      disabled={exporting || !exportPath.trim()}
                      className="gap-2"
                    >
                      {exporting ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <FileDown className="h-4 w-4" />
                      )}
                      {t("settings.diagnostics.export")}
                    </Button>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {t("settings.diagnostics.exportHint")}
                  </p>
                </div>
              </>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => handleOpenChange(false)}>
              {t("common.cancel")}
            </Button>
            {!preview ? (
              <Button
                onClick={() => void generatePreview()}
                disabled={generating}
                className="gap-2"
              >
                {generating && <Loader2 className="h-4 w-4 animate-spin" />}
                {t("settings.diagnostics.preview")}
              </Button>
            ) : (
              <>
                <Button variant="outline" onClick={() => setPreview(null)}>
                  {t("settings.diagnostics.edit")}
                </Button>
                {preview.issue_url && (
                  <Button onClick={openIssue} className="gap-2">
                    <Send className="h-4 w-4" />
                    {t("settings.diagnostics.send")}
                  </Button>
                )}
              </>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
