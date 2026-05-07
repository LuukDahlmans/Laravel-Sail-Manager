import type { Template } from './types';

class UIState {
  showCreateModal = $state(false);
  showImportModal = $state(false);
  showCloneModal = $state(false);
  pendingTemplate = $state<Template | null>(null);
}

export const ui = new UIState();
