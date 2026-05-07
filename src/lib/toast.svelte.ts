export type ToastType = 'error' | 'success' | 'warning' | 'info';

export interface ToastAction {
  label: string;
  handler: () => void | Promise<void>;
}

export interface Toast {
  id: string;
  type: ToastType;
  title?: string;
  message: string;
  duration: number;
  createdAt: number;
  action?: ToastAction;
}

interface ShowOpts {
  type?: ToastType;
  title?: string;
  message: string;
  duration?: number;
  action?: ToastAction;
}

class ToastStore {
  toasts = $state<Toast[]>([]);

  private id() {
    return (
      (typeof crypto !== 'undefined' && crypto.randomUUID?.()) ||
      Math.random().toString(36).slice(2)
    );
  }

  show(opts: ShowOpts): string {
    const t: Toast = {
      id: this.id(),
      type: opts.type ?? 'info',
      title: opts.title,
      message: opts.message,
      duration: opts.duration ?? defaultDuration(opts.type ?? 'info'),
      createdAt: Date.now(),
      action: opts.action,
    };
    this.toasts = [...this.toasts, t];
    if (t.duration > 0) {
      window.setTimeout(() => this.dismiss(t.id), t.duration);
    }
    return t.id;
  }

  dismiss(id: string) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  clear() {
    this.toasts = [];
  }

  error(message: string, title?: string) {
    return this.show({ type: 'error', message, title, duration: 7000 });
  }

  success(message: string, title?: string) {
    return this.show({ type: 'success', message, title, duration: 3000 });
  }

  warning(message: string, title?: string) {
    return this.show({ type: 'warning', message, title, duration: 5000 });
  }

  info(message: string, title?: string) {
    return this.show({ type: 'info', message, title, duration: 4000 });
  }
}

function defaultDuration(type: ToastType) {
  switch (type) {
    case 'error':
      return 7000;
    case 'warning':
      return 5000;
    case 'success':
      return 3000;
    default:
      return 4000;
  }
}

export const toast = new ToastStore();
