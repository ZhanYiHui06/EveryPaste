/**
 * EveryPaste - Main Application Logic
 * 
 * Handles frontend UI interactions and backend communication
 */

// Tauri API
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;
const { open: openExternal } = window.__TAURI__.shell;

// ============== State Management ==============
const state = {
    items: [],           // Clipboard history list
    selectedIndex: 0,    // Currently selected index
    searchQuery: '',     // Search keyword
    settings: null,      // User settings
    contextMenuItemId: null,  // Context menu associated item ID
    isRecordingShortcut: false, // Whether recording shortcut
    isWindowVisible: false, // Window visibility state tracking
};

// ============== DOM Element References ==============
const elements = {
    app: document.getElementById('app'),
    clipboardList: document.getElementById('clipboard-list'),
    emptyState: document.getElementById('empty-state'),
    searchInput: document.getElementById('search-input'),
    settingsPanel: document.getElementById('settings-panel'),
    contextMenu: document.getElementById('context-menu'),
    toast: document.getElementById('toast'),
    toastMessage: document.getElementById('toast-message'),

    // Buttons
    btnSettings: document.getElementById('btn-settings'),
    btnClose: document.getElementById('btn-close'),
    btnBack: document.getElementById('btn-back'),
    btnClearAll: document.getElementById('btn-clear-all'),

    // Settings controls
    storageLimitSelect: document.getElementById('storage-limit-select'),
    storageLimitValue: document.getElementById('storage-limit-value'),
    autoStart: document.getElementById('auto-start'),

    shortcutDisplay: document.getElementById('shortcut-display'),

    // System integration buttons
    btnFixWinV: document.getElementById('btn-fix-win-v'),
    btnRestoreWinV: document.getElementById('btn-restore-win-v'),

    // Welcome panel
    welcomePanel: document.getElementById('welcome-panel'),
    btnStart: document.getElementById('btn-start'),

    // Modal
    modalOverlay: document.getElementById('modal-overlay'),
    modalTitle: document.getElementById('modal-title'),
    modalMessage: document.getElementById('modal-message'),
    modalBtnConfirm: document.getElementById('modal-btn-confirm'),
    modalBtnCancel: document.getElementById('modal-btn-cancel'),
};

// ============== Utility Functions ==============

/**
 * Show Toast notification
 */
function showToast(message, duration = 2000) {
    elements.toastMessage.textContent = message;
    elements.toast.classList.remove('hidden', 'hiding');

    setTimeout(() => {
        elements.toast.classList.add('hiding');
        setTimeout(() => {
            elements.toast.classList.add('hidden');
            elements.toast.classList.remove('hiding');
        }, 200);
    }, duration);
}

/**
 * Show confirmation dialog (Promise)
 */
function showConfirmDialog(title, message, confirmText = '确定', cancelText = '取消') {
    return new Promise((resolve) => {
        elements.modalTitle.textContent = title;
        elements.modalMessage.innerHTML = message.replace(/\n/g, '<br>');
        elements.modalBtnConfirm.textContent = confirmText;
        elements.modalBtnCancel.textContent = cancelText;

        // Remove hidden class before showing, but keep opacity 0
        elements.modalOverlay.classList.remove('hidden');

        // Force reflow to ensure transition works
        void elements.modalOverlay.offsetWidth;

        // Add showing class to trigger animation
        elements.modalOverlay.classList.add('showing');

        // Bind one-time events
        const cleanup = () => {
            elements.modalOverlay.classList.remove('showing');
            // Wait for animation to finish before hiding
            setTimeout(() => {
                elements.modalOverlay.classList.add('hidden');
                elements.modalBtnConfirm.onclick = null;
                elements.modalBtnCancel.onclick = null;
            }, 300);
        };

        elements.modalBtnConfirm.onclick = () => {
            cleanup();
            resolve(true);
        };

        elements.modalBtnCancel.onclick = () => {
            cleanup();
            resolve(false);
        };
    });
}

/**
 * Format time as relative time
 */
function formatRelativeTime(dateStr) {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now - date;
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHour / 24);

    if (diffSec < 60) return 'Just now';
    if (diffMin < 60) return `${diffMin}m ago`;
    if (diffHour < 24) return `${diffHour}h ago`;
    if (diffDay < 7) return `${diffDay}d ago`;

    return date.toLocaleDateString('zh-CN');
}

/**
 * Get content type display name
 */
function getContentTypeName(type) {
    const names = {
        'text': 'Text',
        'rich_text': 'Rich Text',
        'image': 'Image',
    };
    return names[type] || '未知';
}

/**
 * Get content type icon SVG
 */
function getContentTypeIcon(type) {
    const icons = {
        'text': `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
      <polyline points="14 2 14 8 20 8"/>
      <line x1="16" y1="13" x2="8" y2="13"/>
      <line x1="16" y1="17" x2="8" y2="17"/>
      <polyline points="10 9 9 9 8 9"/>
    </svg>`,
        'rich_text': `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
      <polyline points="14 2 14 8 20 8"/>
      <line x1="16" y1="13" x2="8" y2="13"/>
      <line x1="16" y1="17" x2="8" y2="17"/>
    </svg>`,
        'image': `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
      <circle cx="8.5" cy="8.5" r="1.5"/>
      <polyline points="21 15 16 10 5 21"/>
    </svg>`,
    };
    return icons[type] || icons['text'];
}

// ============== Window Animation ==============

/**
 * Play window show animation (with state tracking)
 */
function playShowAnimation() {
    elements.app.classList.remove('window-hide');
    elements.app.classList.add('window-show');
    state.isWindowVisible = true;
}

/**
 * Play window hide animation (Promise)
 */
async function playHideAnimation() {
    return new Promise((resolve) => {
        elements.app.classList.remove('window-show');
        elements.app.classList.add('window-hide');
        // Wait for animation to finish
        setTimeout(resolve, 200);
    });
}

// ============== Render Functions ==============

/**
 * Render clipboard list
 */
function renderClipboardList() {
    const items = state.items;

    if (items.length === 0) {
        elements.clipboardList.classList.add('hidden');
        elements.emptyState.classList.remove('hidden');
        return;
    }

    elements.clipboardList.classList.remove('hidden');
    elements.emptyState.classList.add('hidden');

    const html = items.map((item, index) => {
        const isSelected = index === state.selectedIndex;
        const iconClass = item.content_type === 'image' ? 'image-icon' :
            item.content_type === 'rich_text' ? 'rich-text-icon' : '';

        let iconHtml;
        if (item.content_type === 'image') {
            const hasThumb = !!item.image_thumbnail;
            console.log(`[BasicDebug] Rendering image item ${item.id}: hasThumb=${hasThumb}, len=${item.image_thumbnail?.length}`);

            if (item.image_thumbnail) {
                console.error(`[ThumbRender] Item ${item.id} has thumbnail, len=${item.image_thumbnail.length}`);
                iconHtml = `<img src="${item.image_thumbnail}" alt="Thumb" class="item-thumbnail" style="display: block; width: 36px; height: 36px; object-fit: cover; border-radius: 6px; flex-shrink: 0;">`;
            } else {
                console.warn('Image item missing thumbnail:', item.id);
                iconHtml = `<div class="item-icon ${iconClass}">${getContentTypeIcon(item.content_type)}</div>`;
            }
        } else {
            iconHtml = `<div class="item-icon ${iconClass}">${getContentTypeIcon(item.content_type)}</div>`;
        }

        return `
      <div class="clipboard-item ${isSelected ? 'selected' : ''}" 
           data-id="${item.id}" 
           data-index="${index}">
        ${iconHtml}
        <div class="item-content">
          <div class="item-preview">${escapeHtml(item.preview)}</div>
          <div class="item-meta">
            <span class="item-type">${getContentTypeName(item.content_type)}</span>
            <span class="item-time">· ${formatRelativeTime(item.created_at)}</span>
          </div>
        </div>
      </div>
    `;
    }).join('');

    elements.clipboardList.innerHTML = html;
    scrollToSelected();
}

/**
 * HTML escape
 */
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

/**
 * Scroll to selected item
 */
function scrollToSelected() {
    const selectedEl = elements.clipboardList.querySelector('.clipboard-item.selected');
    if (selectedEl) {
        selectedEl.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
}

/**
 * Update selected index
 */
function updateSelectedIndex(newIndex) {
    if (state.items.length === 0) return;

    if (newIndex < 0) newIndex = 0;
    if (newIndex >= state.items.length) newIndex = state.items.length - 1;

    state.selectedIndex = newIndex;

    const items = elements.clipboardList.querySelectorAll('.clipboard-item');
    items.forEach((el, idx) => {
        el.classList.toggle('selected', idx === newIndex);
    });

    scrollToSelected();
}

// ============== API Calls ==============

/**
 * Load clipboard history
 */
async function loadClipboardHistory() {
    try {
        const result = await invoke('get_clipboard_history', { limit: null });
        if (result.success) {
            state.items = result.data;
            state.selectedIndex = 0;
            renderClipboardList();
        } else {
            console.error('Failed to load clipboard history:', result.error);
        }
    } catch (error) {
        console.error('Failed to load clipboard history:', error);
    }
}

/**
 * Search clipboard
 */
async function searchClipboard(query) {
    try {
        const result = await invoke('search_clipboard', { query, limit: null });
        if (result.success) {
            state.items = result.data;
            state.selectedIndex = 0;
            renderClipboardList();
        }
    } catch (error) {
        console.error('Search failed:', error);
    }
}

/**
 * Paste specified item
 */
async function pasteItem(id, asPlainText = false) {
    try {
        const result = await invoke('paste_item', { id, asPlainText });
        if (result.success) {
            // Call backend to restore focus and auto-paste
            try {
                await invoke('restore_and_paste');
            } catch (e) {
                console.warn('restore_and_paste failed, falling back to hideWindow:', e);
                await hideWindow();
            }
        } else {
            showToast('Paste failed: ' + result.error);
        }
    } catch (error) {
        console.error('Paste failed:', error);
        showToast('Paste failed');
    }
}

/**
 * Hide window (with animation and robustness handling)
 */
async function hideWindow() {
    if (!state.isWindowVisible) return;

    try {
        // Play animation first
        const animationPromise = playHideAnimation();
        // Set timeout protection to prevent animation freeze
        const timeoutPromise = new Promise(resolve => setTimeout(resolve, 250));
        await Promise.race([animationPromise, timeoutPromise]);

        // Force call backend hide (more reliable than frontend hide)
        await invoke('hide_main_window');

        // Mark as invisible
        state.isWindowVisible = false;

        // Delay removing animation class
        setTimeout(() => {
            elements.app.classList.remove('window-show', 'window-hide');
        }, 50);

    } catch (e) {
        console.warn('Hide window failed:', e);
        // Fallback attempt
        try { await getCurrentWindow().hide(); } catch (_) { }
    }
}

/**
 * Delete specified item
 */
async function deleteItem(id) {
    try {
        const result = await invoke('delete_item', { id });
        if (result.success) {
            await loadClipboardHistory();
            showToast('已删除');
        }
    } catch (error) {
        console.error('Delete failed:', error);
    }
}

/**
 * Clear all history
 */
async function clearAllHistory() {
    if (!await showConfirmDialog('确认清空', '确定要清空所有记录吗？<br>此操作不可撤销。', '清空')) {
        return;
    }

    try {
        const result = await invoke('clear_all_history');
        if (result.success) {
            await loadClipboardHistory();
            showToast('已清空所有历史记录');
        }
    } catch (error) {
        console.error('Clear all failed:', error);
    }
}

/**
 * Load settings
 */
async function loadSettings() {
    try {
        const result = await invoke('get_settings');
        if (result.success) {
            state.settings = result.data;
            applySettings(state.settings);
        }
    } catch (error) {
        console.error('Failed to load settings:', error);
    }
}

/**
 * Apply settings to UI
 */
function applySettings(settings) {
    setTheme(settings.theme);
    updateStorageLimitDisplay(settings.storage_limit);

    if (elements.autoStart) {
        elements.autoStart.checked = settings.auto_start;
    }


    if (settings.shortcut && elements.shortcutDisplay) {
        elements.shortcutDisplay.textContent = settings.shortcut;
    }

    document.querySelectorAll('.theme-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.theme === settings.theme);
    });
}

/**
 * Update storage limit display
 */
function updateStorageLimitDisplay(value) {
    if (!elements.storageLimitValue || !elements.storageLimitSelect) return;

    const text = value === -1 ? '不限制' : `最近 ${value} 条`;
    elements.storageLimitValue.textContent = text;

    const options = elements.storageLimitSelect.querySelectorAll('.custom-select-option');
    options.forEach(opt => {
        opt.classList.toggle('selected', opt.dataset.value === value.toString());
    });
}

/**
 * Set theme
 */
function setTheme(theme) {
    const lightTheme = document.getElementById('theme-light');
    const darkTheme = document.getElementById('theme-dark');

    if (theme === 'dark') {
        lightTheme.disabled = true;
        darkTheme.disabled = false;
    } else {
        lightTheme.disabled = false;
        darkTheme.disabled = true;
    }
}

/**
 * Save settings
 */
async function saveSettings(updates) {
    try {
        const result = await invoke('update_settings', { updates });
        if (result.success) {
            state.settings = result.data;
            applySettings(state.settings);
        }
    } catch (error) {
        console.error('Failed to save settings:', error);
    }
}


async function setWinVPolicy(enable) {
    const action = enable ? '恢复' : '禁用';
    const message = enable
        ? '确定要恢复系统剪贴板吗？<br>可能需要重启生效。'
        : '此操作将通过注册表永久禁用系统剪贴板以支持 Win+V。<br>需要管理员权限，建议操作后重启。';

    if (!await showConfirmDialog(action + '系统剪贴板', message)) return;

    try {
        await invoke('set_win_v_policy', { enable });
        showToast('命令已发送，请在弹出的 PowerShell 窗口中允许提权');
        // Delayed restart prompt
        setTimeout(async () => {
            await showConfirmDialog('建议重启', '操作完成。<br>建议重启电脑以使更改生效。', '好的', '关闭');
        }, 1500);
    } catch (e) {
        console.error(e);
        showToast('操作失败: ' + e);
    }
}

// ============== First Run Welcome Page ==============

/**
 * Check if first run and show welcome page
 */
async function checkFirstRun() {
    try {
        const result = await invoke('is_first_run');
        if (result.success && result.data === true) {
            showWelcomePanel();
        }
    } catch (error) {
        console.error('Failed to check first run:', error);
    }
}

/**
 * Show welcome panel
 */
function showWelcomePanel() {
    if (!elements.welcomePanel) return;

    elements.welcomePanel.classList.remove('hidden', 'closing');
    elements.welcomePanel.classList.add('opening');

    // Remove opening class after animation ends
    setTimeout(() => {
        elements.welcomePanel.classList.remove('opening');
    }, 400);
}

/**
 * Close welcome page and open settings
 */
async function closeWelcomeAndOpenSettings() {
    if (!elements.welcomePanel) return;

    // Mark first run as complete
    try {
        await invoke('complete_first_run');
    } catch (error) {
        console.error('Failed to mark first run completed:', error);
    }

    // Close welcome panel animation
    elements.welcomePanel.classList.add('closing');

    setTimeout(() => {
        elements.welcomePanel.classList.add('hidden');
        elements.welcomePanel.classList.remove('closing');

        // Open settings page
        showSettingsPanel();
    }, 300);
}


// ============== Custom Select ==============

function initCustomSelect() {
    const select = elements.storageLimitSelect;
    if (!select) return;

    const trigger = select.querySelector('.custom-select-trigger');
    const options = select.querySelectorAll('.custom-select-option');

    trigger.addEventListener('click', (e) => {
        e.stopPropagation();
        select.classList.toggle('open');
    });

    options.forEach(option => {
        option.addEventListener('click', (e) => {
            e.stopPropagation();
            const value = parseInt(option.dataset.value);
            saveSettings({ storage_limit: value });
            select.classList.remove('open');
        });
    });

    document.addEventListener('click', () => {
        select.classList.remove('open');
    });
}

// ============== Shortcut Recording ==============

function initShortcutRecording() {
    const display = elements.shortcutDisplay;
    if (!display) return;

    display.addEventListener('click', () => {
        if (state.isRecordingShortcut) return;
        startRecordingShortcut();
    });

    display.addEventListener('keydown', (e) => {
        if (!state.isRecordingShortcut) return;
        e.preventDefault();
        handleShortcutKey(e);
    });

    display.addEventListener('blur', () => {
        if (state.isRecordingShortcut) {
            stopRecordingShortcut(false);
        }
    });
}

function startRecordingShortcut() {
    state.isRecordingShortcut = true;
    elements.shortcutDisplay.classList.add('recording');
    elements.shortcutDisplay.textContent = '按下快捷键...';
    elements.shortcutDisplay.focus();
}

function stopRecordingShortcut(save = false) {
    state.isRecordingShortcut = false;
    elements.shortcutDisplay.classList.remove('recording');

    if (!save && state.settings?.shortcut) {
        elements.shortcutDisplay.textContent = state.settings.shortcut;
    }
}

function handleShortcutKey(e) {
    const parts = [];

    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Win');

    const modifierKeys = ['Control', 'Alt', 'Shift', 'Meta'];
    if (!modifierKeys.includes(e.key) && parts.length > 0) {
        let key = e.key.toUpperCase();
        if (e.code.startsWith('Key')) {
            key = e.code.replace('Key', '');
        } else if (e.code.startsWith('Digit')) {
            key = e.code.replace('Digit', '');
        }
        parts.push(key);

        const shortcut = parts.join('+');
        elements.shortcutDisplay.textContent = shortcut;
        stopRecordingShortcut(true);

        saveSettings({ shortcut });
        showToast(`快捷键已设置为 ${shortcut}`);
    }
}

// ============== Event Handling ==============

/**
 * Initialize event listeners
 */
function initEventListeners() {
    // Auto-hide on blur (frontend implementation with debounce)
    window.addEventListener('blur', () => {
        if (state.isWindowVisible) {
            setTimeout(() => {
                // If document no longer has focus (ensure focus hasn't moved to internal element)
                if (!document.hasFocus()) {
                    hideWindow();
                }
            }, 100);
        }
    });

    // Search input
    let searchTimeout;
    elements.searchInput.addEventListener('input', (e) => {
        clearTimeout(searchTimeout);
        state.searchQuery = e.target.value;
        searchTimeout = setTimeout(() => {
            if (state.searchQuery) {
                searchClipboard(state.searchQuery);
            } else {
                loadClipboardHistory();
            }
        }, 200);
    });

    // Keyboard navigation
    document.addEventListener('keydown', handleKeyDown);

    // Click clipboard item
    elements.clipboardList.addEventListener('click', (e) => {
        const item = e.target.closest('.clipboard-item');
        if (item) {
            const id = parseInt(item.dataset.id);
            pasteItem(id, false);
        }
    });

    // Disable global browser context menu
    document.addEventListener('contextmenu', (e) => {
        e.preventDefault();
    });

    // Context menu
    elements.clipboardList.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        const item = e.target.closest('.clipboard-item');
        if (item) {
            state.contextMenuItemId = parseInt(item.dataset.id);
            showContextMenu(e.clientX, e.clientY);
        }
    });

    // Context menu item click
    elements.contextMenu.addEventListener('click', (e) => {
        e.stopPropagation(); // Prevent event bubbling to document click handler
        const menuItem = e.target.closest('.context-menu-item');
        if (menuItem) {
            const action = menuItem.dataset.action;
            handleContextMenuAction(action);
        }
    });

    // Click outside to close context menu
    document.addEventListener('click', (e) => {
        if (!elements.contextMenu.contains(e.target)) {
            hideContextMenu();
        }
    });

    // Settings button
    elements.btnSettings.addEventListener('click', showSettingsPanel);
    elements.btnBack.addEventListener('click', hideSettingsPanel);

    // Close button
    elements.btnClose.addEventListener('click', async (e) => {
        e.preventDefault();
        e.stopPropagation();
        await hideWindow();
    });

    // Theme toggle
    document.querySelectorAll('.theme-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            saveSettings({ theme: btn.dataset.theme });
        });
    });

    // Auto start
    if (elements.autoStart) {
        elements.autoStart.addEventListener('change', (e) => {
            saveSettings({ auto_start: e.target.checked });
        });
    }


    // Win+V system integration
    if (elements.btnFixWinV) {
        elements.btnFixWinV.addEventListener('click', () => setWinVPolicy(false));
    }
    if (elements.btnRestoreWinV) {
        elements.btnRestoreWinV.addEventListener('click', () => setWinVPolicy(true));
    }

    // Clear history
    elements.btnClearAll.addEventListener('click', clearAllHistory);

    // Initialize custom select
    initCustomSelect();

    // Welcome page button
    if (elements.btnStart) {
        elements.btnStart.addEventListener('click', closeWelcomeAndOpenSettings);
    }

    // GitHub link
    const githubLink = document.querySelector('.github-link');
    if (githubLink) {
        githubLink.addEventListener('click', (e) => {
            e.preventDefault();
            openExternal('https://github.com/ZhanYiHui06/EveryPaste');
        });
    }

    // Initialize shortcut recording
    initShortcutRecording();
}

/**
 * Keyboard event handler
 */
function handleKeyDown(e) {
    if (!elements.settingsPanel.classList.contains('hidden')) {
        if (e.key === 'Escape') {
            hideSettingsPanel();
        }
        return;
    }

    if (state.isRecordingShortcut) return;

    switch (e.key) {
        case 'ArrowUp':
            e.preventDefault();
            updateSelectedIndex(state.selectedIndex - 1);
            break;

        case 'ArrowDown':
            e.preventDefault();
            updateSelectedIndex(state.selectedIndex + 1);
            break;

        case 'Enter':
            e.preventDefault();
            if (state.items.length > 0) {
                const item = state.items[state.selectedIndex];
                pasteItem(item.id, e.shiftKey);
            }
            break;

        case 'Escape':
            e.preventDefault();
            hideWindow();
            break;

        case 'Delete':
        case 'Backspace':
            if (document.activeElement !== elements.searchInput && state.items.length > 0) {
                e.preventDefault();
                const item = state.items[state.selectedIndex];
                deleteItem(item.id);
            }
            break;
    }
}

/**
 * Show context menu
 */
function showContextMenu(x, y) {
    // Set position and show menu
    elements.contextMenu.style.left = `${x}px`;
    elements.contextMenu.style.top = `${y}px`;
    elements.contextMenu.classList.remove('hidden');

    // Get menu dimensions and #app container bounds
    const rect = elements.contextMenu.getBoundingClientRect();
    const appRect = elements.app.getBoundingClientRect();
    const padding = 8; // Safe distance from edge

    // Check right boundary overflow
    if (rect.right > appRect.right - padding) {
        elements.contextMenu.style.left = `${appRect.right - rect.width - padding}px`;
    }
    // Check bottom boundary overflow
    if (rect.bottom > appRect.bottom - padding) {
        elements.contextMenu.style.top = `${appRect.bottom - rect.height - padding}px`;
    }
    // Check left boundary overflow
    if (rect.left < appRect.left + padding) {
        elements.contextMenu.style.left = `${appRect.left + padding}px`;
    }
    // Check top boundary overflow
    if (rect.top < appRect.top + padding) {
        elements.contextMenu.style.top = `${appRect.top + padding}px`;
    }
}

/**
 * Hide context menu
 */
function hideContextMenu() {
    elements.contextMenu.classList.add('hidden');
    state.contextMenuItemId = null;
}

/**
 * Handle context menu action
 */
function handleContextMenuAction(action) {
    // Save ID first as hideContextMenu will clear it
    const itemId = state.contextMenuItemId;
    hideContextMenu();

    if (!itemId) return;

    switch (action) {
        case 'paste':
            pasteItem(itemId, false);
            break;
        case 'paste-plain':
            pasteItem(itemId, true);
            break;
        case 'delete':
            deleteItem(itemId);
            break;
    }
}

/**
 * Show settings panel (with animation)
 */
function showSettingsPanel() {
    elements.settingsPanel.classList.remove('hidden', 'closing');
    elements.settingsPanel.classList.add('opening');
    setTimeout(() => {
        elements.settingsPanel.classList.remove('opening');
    }, 300);
}

/**
 * Hide settings panel (with animation)
 */
function hideSettingsPanel() {
    elements.settingsPanel.classList.add('closing');
    setTimeout(() => {
        elements.settingsPanel.classList.add('hidden');
        elements.settingsPanel.classList.remove('closing');
    }, 250);
}

/**
 * Initialize Tauri event listeners
 */
async function initTauriListeners() {
    await listen('clipboard-updated', () => {
        loadClipboardHistory();
    });

    await listen('window-shown', () => {
        playShowAnimation();
        elements.searchInput.focus();
    });

    await listen('focus-first-item', () => {
        loadClipboardHistory().then(() => {
            state.selectedIndex = 0;
            renderClipboardList();
            elements.searchInput.focus();
        });
    });

    await listen('open-settings', () => {
        showSettingsPanel();
    });
}

// ============== Application Initialization ==============

async function init() {
    console.log('EveryPaste initializing...');
    await loadSettings();
    await loadClipboardHistory();
    initEventListeners();
    await initTauriListeners();

    // Check first run and show welcome page
    await checkFirstRun();

    console.log('EveryPaste initialized');
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
} else {
    init();
}
