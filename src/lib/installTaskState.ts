type InstallTaskLike = {
  task_id: string;
  stage: string;
  result: unknown | null;
};

const STAGE_ORDER: Readonly<Record<string, number>> = {
  preflight: 0,
  awaiting_confirmation: 1,
  repairing: 2,
  installing_tools: 3,
  verifying: 4,
  completed: 5,
};

export function mergeInstallTaskSnapshot<T extends InstallTaskLike>(
  current: T | null,
  incoming: T,
): T {
  if (!current || current.task_id !== incoming.task_id) return incoming;
  if (current.result && !incoming.result) return current;
  const currentStage = STAGE_ORDER[current.stage] ?? -1;
  const incomingStage = STAGE_ORDER[incoming.stage] ?? -1;
  return incomingStage < currentStage ? current : incoming;
}
