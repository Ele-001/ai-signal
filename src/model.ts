export const statuses = ['attention', 'repairing', 'running', 'completed', 'idle'] as const;
export type Status = typeof statuses[number];
export type Task = { id: string; title: string; status: Status; source: string; message?: string; updatedAt: number; external?: boolean };
export type SignalStyle = 'classic' | 'silver' | 'mechanical';
export type Settings = { style: SignalStyle; motion: boolean; alwaysOnTop: boolean; scale: number; opacity: number; sound: boolean };
export type LocalState = { version: 1; tasks: Task[]; settings: Settings };
export const info: Record<Status, {label: string}> = {
  attention: {label: '需要处理'},
  repairing: {label: 'AI 正在修复'},
  running: {label: '正在运行'},
  completed: {label: '已完成'},
  idle: {label: '空闲'},
};
export const defaults: LocalState = {version: 1, tasks: [], settings: {style: 'classic', motion: true, alwaysOnTop: true, scale: 1, opacity: 1, sound: false}};
export function aggregate(tasks: Task[]): Status {
  return statuses.find(status => tasks.some(task => task.status === status)) ?? 'idle';
}
export function validTask(value: unknown): value is Task {
  if (!value || typeof value !== 'object') return false;
  const t = value as Task;
  return typeof t.id === 'string' && t.id.length > 0 && t.id.length <= 256 && typeof t.title === 'string' && t.title.length > 0 && [...t.title].length <= 200 && statuses.includes(t.status) && typeof t.source === 'string' && [...t.source].length <= 100 && typeof t.updatedAt === 'number' && Number.isFinite(t.updatedAt) && (t.message === undefined || typeof t.message === 'string');
}
export function restore(raw: string | null): LocalState {
  try {
    const data = JSON.parse(raw ?? 'null');
    if (data?.version !== 1) return structuredClone(defaults);
    const s = data.settings ?? {};
    return {version: 1, tasks: Array.isArray(data.tasks) ? data.tasks.filter(validTask).slice(0, 100) : [], settings: {
      style: s.style === 'silver' || s.style === 'mechanical' ? s.style : 'classic',
      motion: typeof s.motion === 'boolean' ? s.motion : true,
      alwaysOnTop: typeof s.alwaysOnTop === 'boolean' ? s.alwaysOnTop : true,
      sound: typeof s.sound === 'boolean' ? s.sound : false,
      scale: typeof s.scale === 'number' && Number.isFinite(s.scale) ? Math.min(1.6, Math.max(.7, s.scale)) : 1,
      opacity: typeof s.opacity === 'number' && Number.isFinite(s.opacity) ? Math.min(1, Math.max(.4, s.opacity)) : 1,
    }};
  } catch { return structuredClone(defaults); }
}
export function mergeTask(tasks: Task[], task: Task): Task[] {
  const old = tasks.find(t => t.id === task.id);
  if (old && old.updatedAt > task.updatedAt) return tasks;
  return [...tasks.filter(t => t.id !== task.id), task].slice(-512);
}
export function sameTasks(a: Task[], b: Task[]): boolean {
  if (a.length !== b.length) return false;
  const previous = new Map(a.map(task => [task.id, task]));
  return b.every(task => {
    const old = previous.get(task.id);
    return old && old.title === task.title && old.status === task.status && old.source === task.source && old.message === task.message && old.updatedAt === task.updatedAt;
  });
}
