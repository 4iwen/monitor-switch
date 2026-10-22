use crate::ui::UiState;

pub fn settings_html(state: &UiState) -> String {
    let state_json = serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string());
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <style>{base}{settings}</style>
</head>
<body class="{platform} settings-body">
  <main id="app"></main>
  <script>
    const ICONS = {icons};
    const send = (type, payload = {{}}) => window.ipc.postMessage(JSON.stringify({{ type, ...payload }}));
    const icon = name => ICONS[name] || ICONS.monitor;
    const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, char => ({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[char]));
    let activeMonitor = 0;
    let isDirty = false;
    let isSaving = false;
    const shortcutFields = () => [...document.querySelectorAll('[data-shortcut]')];
    const shortcutKey = field => `${{field.dataset.display}}:${{field.dataset.source}}`;
    const shortcutLabel = value => value.replaceAll('Control', 'Ctrl').replaceAll('Super', document.body.classList.contains('macos') ? 'Cmd' : 'Win').replaceAll('Key', '').replaceAll('Digit', '').replaceAll('+', ' + ');
    const shortcutFor = (state, display, value) => state.shortcuts.find(binding => binding.display === display && binding.value === value)?.shortcut || '';
    function shortcutControl(state, monitor, value) {{
      const binding = shortcutFor(state, monitor.selector, value);
      return `<span class="shortcut-control"><input readonly data-shortcut data-display="${{escapeHtml(monitor.selector)}}" data-source="${{value ?? ''}}" data-binding="${{escapeHtml(binding)}}" data-default-binding="${{escapeHtml(binding)}}" value="${{escapeHtml(shortcutLabel(binding))}}" placeholder="快捷键未设置" aria-label="${{escapeHtml(monitor.name)}} ${{value == null ? '切回上一个输入源' : escapeHtml(monitor.available_inputs.find(input => input.value === value)?.subtitle || value)}} 快捷键"><button type="button" class="clear-shortcut" aria-label="清除快捷键" title="清除快捷键" onclick="clearShortcut(this)">×</button></span>`;
    }}

    const rowKey = row => `${{row.querySelector('[data-value]').dataset.display}}:${{row.querySelector('[data-value]').dataset.value}}`;
    const pendingEdits = () => new Map([...document.querySelectorAll('[data-input-row]')].filter(row => {{
      const name = row.querySelector('[data-value]');
      const visibility = row.querySelector('[data-visibility]');
      return name.value.trim() !== name.dataset.default || String(visibility.checked) !== visibility.dataset.defaultVisible;
    }}).map(row => [rowKey(row), {{ name: row.querySelector('[data-value]').value, visible: row.querySelector('[data-visibility]').checked }}]));

    const structureKey = state => state.monitors.map(monitor => [
      monitor.selector,
      ...monitor.available_inputs.map(input => `${{input.value}}:${{input.subtitle}}:${{input.reported_supported}}`)
    ]).flat().join('|');

    function updateLiveState(state) {{
      const configError = document.getElementById('config-error');
      if (configError) {{ configError.textContent = state.config_error || ''; configError.hidden = !state.config_error; }}
      const error = document.getElementById('shortcut-error');
      if (error) {{ error.textContent = state.shortcut_error || ''; error.hidden = !state.shortcut_error; }}
      state.monitors.forEach((monitor, monitorIndex) => {{
        const status = document.querySelector(`[data-display-state="${{monitorIndex}}"]`);
        if (!status) return;
        status.textContent = `${{monitor.available_inputs.filter(input => input.show_in_menu).length}} 个菜单输入源`;
      }});
      if (isDirty) return;
      shortcutFields().forEach(field => {{
        const binding = shortcutFor(state, field.dataset.display, field.dataset.source === '' ? null : Number(field.dataset.source));
        field.dataset.binding = binding;
        field.dataset.defaultBinding = binding;
        field.value = shortcutLabel(binding);
      }});
      document.querySelectorAll('[data-input-row]').forEach(row => {{
        const monitor = state.monitors[Number(row.dataset.monitorIndex)];
        const name = row.querySelector('[data-value]');
        const input = monitor?.available_inputs.find(candidate => candidate.value === Number(name.dataset.value));
        if (!input) return;
        const visibility = row.querySelector('[data-visibility]');
        name.value = input.name;
        name.dataset.default = input.name;
        visibility.checked = input.show_in_menu;
        visibility.dataset.defaultVisible = String(input.show_in_menu);
      }});
      updateDirtyState();
    }}

    function render(state) {{
      window.__STATE__ = state;
      activeMonitor = Math.min(activeMonitor, Math.max(0, state.monitors.length - 1));
      const app = document.getElementById('app');
      const key = structureKey(state);
      if (app.dataset.structureKey === key && app.firstElementChild) {{
        updateLiveState(state);
        return;
      }}
      const edits = pendingEdits();
      if (document.activeElement?.matches('[data-shortcut]')) send('recordShortcut', {{ active: false }});
      const shortcutDrafts = new Map(shortcutFields().filter(field => field.dataset.binding !== field.dataset.defaultBinding).map(field => [shortcutKey(field), field.dataset.binding]));
      const monitorTabs = state.monitors.map((monitor, monitorIndex) => `
        <button class="monitor-tab ${{monitorIndex === activeMonitor ? 'active' : ''}}" data-monitor-tab="${{monitorIndex}}" id="monitor-tab-${{monitorIndex}}" role="tab" aria-controls="monitor-panel-${{monitorIndex}}" aria-selected="${{monitorIndex === activeMonitor}}" tabindex="${{monitorIndex === activeMonitor ? 0 : -1}}">
          ${{icon('monitor')}}<span>${{escapeHtml(monitor.name)}}</span>
        </button>`).join('');
      const displays = state.monitors.map((monitor, monitorIndex) => {{
        const rows = monitor.available_inputs.map(input => `
            <div class="input-row ${{input.reported_supported ? '' : 'candidate'}}" data-input-row data-monitor-index="${{monitorIndex}}">
              <span class="port-icon">${{icon(input.icon)}}</span>
              <span class="physical-source">
                <strong>${{escapeHtml(input.subtitle)}}</strong>
                <small>${{escapeHtml(input.hex)}} · ${{input.reported_supported ? '显示器报告支持' : '通用候选'}}</small>
              </span>
              <div class="input-editor"><input class="name-field" data-value="${{input.value}}" data-display="${{escapeHtml(monitor.selector)}}" data-display-index="${{monitorIndex}}" data-default="${{escapeHtml(input.name)}}" value="${{escapeHtml(input.name)}}" maxlength="48" aria-label="${{escapeHtml(input.subtitle)}} 的显示名称">${{shortcutControl(state, monitor, input.value)}}</div>
              <label class="menu-toggle" title="显示在状态栏菜单">
                <input type="checkbox" data-visibility data-default-visible="${{input.show_in_menu}}" ${{input.show_in_menu ? 'checked' : ''}} aria-label="在菜单中显示 ${{escapeHtml(input.subtitle)}}">
                <span class="toggle-track"><span></span></span>
              </label>
            </div>`).join('');
        return `
          <section class="display-panel ${{monitorIndex === activeMonitor ? 'active' : ''}}" data-monitor-panel="${{monitorIndex}}" id="monitor-panel-${{monitorIndex}}" role="tabpanel" aria-labelledby="monitor-tab-${{monitorIndex}}">
            <header class="display-heading">
              <span><strong>${{escapeHtml(monitor.name)}}</strong><small>${{escapeHtml(monitor.subtitle)}}</small></span>
              <span class="display-state" data-display-state="${{monitorIndex}}">${{monitor.available_inputs.filter(input => input.show_in_menu).length}} 个菜单输入源</span>
            </header>
            <div class="previous-shortcut"><span>切回上一个输入源</span>${{shortcutControl(state, monitor, null)}}</div>
            <div class="column-heading"><span>输入源</span><span>菜单名称</span><span>显示</span></div>
            <div class="input-list">${{rows}}</div>
          </section>`;
      }}).join('');

      app.innerHTML = `
        <section class="settings-page">
          <header class="settings-header">
            <h1>显示器输入源</h1>
            <div class="monitor-tabs" role="tablist">${{monitorTabs}}</div>
          </header>
          <div class="settings-scroll">${{displays || '<div class="empty">尚未检测到可配置的显示器</div>'}}</div>
          <div id="shortcut-error" class="shortcut-error" role="alert" ${{state.shortcut_error ? '' : 'hidden'}}>${{escapeHtml(state.shortcut_error || '')}}</div>
          <div id="config-error" class="shortcut-error" role="alert" ${{state.config_error ? '' : 'hidden'}}>${{escapeHtml(state.config_error || '')}}</div>
          <footer>
            <span class="backend-status">${{escapeHtml(state.backend)}}</span>
            <div class="actions">
              <div id="notice" class="notice" role="status">${{icon('checkCircle')}}<span>已保存</span></div>
              <button onclick="requestClose()">取消</button>
              <button id="save-button" class="primary" onclick="saveLabels()" disabled>保存</button>
            </div>
          </footer>
        </section>
        <dialog id="discard-dialog" aria-labelledby="discard-title">
          <h2 id="discard-title">放弃未保存的修改？</h2>
          <p>输入源名称、快捷键和菜单显示设置尚未保存。</p>
          <div class="dialog-actions"><button onclick="document.getElementById('discard-dialog').close()">继续编辑</button><button class="destructive" onclick="discardAndClose()">放弃修改</button></div>
        </dialog>`;
      app.dataset.structureKey = key;
      document.querySelectorAll('[data-input-row]').forEach(row => {{
        const draft = edits.get(rowKey(row));
        if (!draft) return;
        row.querySelector('[data-value]').value = draft.name;
        row.querySelector('[data-visibility]').checked = draft.visible;
      }});
      shortcutFields().forEach(field => {{
        const draft = shortcutDrafts.get(shortcutKey(field));
        if (draft === undefined) return;
        field.dataset.binding = draft;
        field.value = shortcutLabel(draft);
      }});
      updateDirtyState();
      document.querySelectorAll('[data-monitor-tab]').forEach(button => button.addEventListener('click', () => selectMonitor(Number(button.dataset.monitorTab))));
      document.querySelector('.monitor-tabs').addEventListener('keydown', event => {{
        const count = state.monitors.length;
        if (!count || !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
        event.preventDefault();
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? count - 1 : (activeMonitor + (event.key === 'ArrowRight' ? 1 : -1) + count) % count;
        selectMonitor(next);
        document.querySelector(`[data-monitor-tab="${{next}}"]`).focus();
      }});
    }}

    function selectMonitor(index) {{
      activeMonitor = index;
      document.querySelectorAll('[data-monitor-tab]').forEach(tab => {{
        const selected = Number(tab.dataset.monitorTab) === index;
        tab.classList.toggle('active', selected);
        tab.setAttribute('aria-selected', String(selected));
        tab.tabIndex = selected ? 0 : -1;
      }});
      document.querySelectorAll('[data-monitor-panel]').forEach(panel => panel.classList.toggle('active', Number(panel.dataset.monitorPanel) === index));
    }}

    function saveLabels() {{
      if (isSaving || !isDirty) return;
      const items = [...document.querySelectorAll('[data-input-row]')]
        .filter(row => {{
          const name = row.querySelector('[data-value]');
          const visibility = row.querySelector('[data-visibility]');
          return name.value.trim() !== name.dataset.default || String(visibility.checked) !== visibility.dataset.defaultVisible;
        }})
        .map(row => {{
          const name = row.querySelector('[data-value]');
          return {{
            value: Number(name.dataset.value),
            display: name.dataset.display || '',
            display_index: Number(name.dataset.displayIndex),
            label: name.value.trim(),
            show_in_menu: row.querySelector('[data-visibility]').checked
          }};
        }});
      isSaving = true;
      updateDirtyState();
      const button = document.getElementById('save-button');
      button.textContent = '保存中…';
      const displayed = new Set(window.__STATE__.monitors.map(monitor => monitor.selector));
      const shortcuts = window.__STATE__.shortcuts.filter(binding => !displayed.has(binding.display));
      shortcutFields().filter(field => field.dataset.binding).forEach(field => shortcuts.push({{
        display: field.dataset.display,
        value: field.dataset.source === '' ? null : Number(field.dataset.source),
        shortcut: field.dataset.binding
      }}));
      send('saveLabels', {{ items, shortcuts }});
    }}

    function requestClose() {{
      if (isSaving) return;
      if (isDirty) document.getElementById('discard-dialog').showModal();
      else send('closeSettings');
    }}

    function discardAndClose() {{
      shortcutFields().forEach(field => {{ field.dataset.binding = field.dataset.defaultBinding; field.value = shortcutLabel(field.dataset.binding); }});
      document.querySelectorAll('[data-input-row]').forEach(row => {{
        const name = row.querySelector('[data-value]');
        const visibility = row.querySelector('[data-visibility]');
        name.value = name.dataset.default;
        visibility.checked = visibility.dataset.defaultVisible === 'true';
      }});
      updateDirtyState();
      document.getElementById('discard-dialog').close();
      send('closeSettings');
    }}

    function updateDirtyState() {{
      const dirty = shortcutFields().some(field => field.dataset.binding !== field.dataset.defaultBinding) || [...document.querySelectorAll('[data-input-row]')].some(row => {{
        const name = row.querySelector('[data-value]');
        const visibility = row.querySelector('[data-visibility]');
        return name.value.trim() !== name.dataset.default || String(visibility.checked) !== visibility.dataset.defaultVisible;
      }});
      const button = document.getElementById('save-button');
      if (button) button.disabled = !dirty || isSaving;
      document.querySelectorAll('.input-row input, [data-shortcut], .clear-shortcut').forEach(control => control.disabled = isSaving);
      isDirty = dirty;
    }}

    window.monitorSwitchRender = render;
    window.monitorSwitchRequestClose = requestClose;
    window.monitorSwitchSaved = () => {{
      isSaving = false;
      const button = document.getElementById('save-button');
      if (button) button.textContent = '保存';
      const notice = document.getElementById('notice');
      if (!notice) return;
      notice.classList.remove('failed');
      notice.querySelector('span').textContent = '已保存';
      notice.classList.add('visible');
      document.querySelectorAll('input[data-value]').forEach(input => input.dataset.default = input.value.trim());
      document.querySelectorAll('[data-visibility]').forEach(input => input.dataset.defaultVisible = String(input.checked));
      shortcutFields().forEach(field => field.dataset.defaultBinding = field.dataset.binding);
      updateDirtyState();
      setTimeout(() => notice.classList.remove('visible'), 2400);
    }};
    window.monitorSwitchSaveError = message => {{
      isSaving = false;
      const button = document.getElementById('save-button');
      if (button) button.textContent = '保存';
      const notice = document.getElementById('notice');
      if (notice) {{ notice.classList.add('visible', 'failed'); notice.querySelector('span').textContent = message; }}
      updateDirtyState();
    }};
    function clearShortcut(button) {{
      const field = button.parentElement.querySelector('input');
      field.dataset.binding = '';
      field.value = '';
      updateDirtyState();
    }}
    document.addEventListener('focusin', event => {{ if (event.target.matches('[data-shortcut]')) send('recordShortcut', {{ active: true }}); }});
    document.addEventListener('focusout', event => {{ if (event.target.matches('[data-shortcut]')) send('recordShortcut', {{ active: false }}); }});
    window.addEventListener('blur', () => send('recordShortcut', {{ active: false }}));
    window.addEventListener('focus', () => send('recordShortcut', {{ active: !!document.activeElement?.matches('[data-shortcut]') }}));
    window.addEventListener('keydown', event => {{
      if (event.target.matches('[data-shortcut]')) {{
        if (event.key === 'Tab') return;
        event.preventDefault();
        if (event.key === 'Escape') {{ event.target.blur(); return; }}
        if (['Control', 'Alt', 'Shift', 'Meta'].includes(event.key) || event.repeat) return;
        const field = event.target;
        if (['Backspace', 'Delete'].includes(event.key) && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey) {{
          field.dataset.binding = ''; field.value = ''; updateDirtyState(); return;
        }}
        const modifiers = [];
        if (event.ctrlKey) modifiers.push('Control');
        if (event.altKey) modifiers.push('Alt');
        if (event.shiftKey) modifiers.push('Shift');
        if (event.metaKey) modifiers.push('Super');
        if (!event.ctrlKey && !event.altKey && !event.metaKey) return;
        field.dataset.binding = [...modifiers, event.code].join('+');
        field.value = shortcutLabel(field.dataset.binding);
        updateDirtyState();
        return;
      }}
      if (event.key === 'Escape' && !document.getElementById('discard-dialog').open) requestClose();
    }});
    document.addEventListener('input', event => {{ if (event.target.matches('input[data-value]')) updateDirtyState(); }});
    document.addEventListener('change', event => {{ if (event.target.matches('[data-visibility]')) updateDirtyState(); }});
    render({state_json});
  </script>
</body>
</html>"#,
        base = base_css(),
        settings = settings_css(),
        icons = icons_js(),
        platform = platform_name(),
        state_json = state_json,
    )
}

fn platform_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else {
        "macos"
    }
}

fn base_css() -> &'static str {
    r#"
:root{color-scheme:light dark;--ease:cubic-bezier(.2,.8,.2,1)}
*{box-sizing:border-box}html,body,#app{margin:0;width:100%;height:100%}body{font-family:-apple-system,BlinkMacSystemFont,"SF Pro Text","Segoe UI Variable Text","Segoe UI",sans-serif;letter-spacing:0;-webkit-font-smoothing:auto;font-synthesis:none}button,input{font:inherit}button{border:0;color:inherit}.separator{height:1px}.connection{display:inline-block;width:8px;height:8px;border-radius:50%;background:#8e8e93}.connection.online{background:#34c759}.empty{padding:32px;text-align:center;color:var(--secondary);font-size:13px}.spinner{width:17px;height:17px;border-radius:50%;border:2px solid rgba(100,100,105,.22);border-top-color:var(--accent);animation:spin .72s linear infinite}.check{font-size:18px;font-weight:700}.error-banner{display:flex;align-items:flex-start;gap:8px;margin:8px 12px 0;padding:9px 10px;border-radius:8px;font-size:12px;line-height:1.35}.error-banner svg{width:16px;height:16px;flex:0 0 auto}.notice{display:flex;align-items:center;gap:8px;opacity:0;pointer-events:none;transition:opacity .14s ease-out}.notice.visible{opacity:1}svg{width:19px;height:19px;stroke:currentColor;fill:none;stroke-width:1.8;stroke-linecap:round;stroke-linejoin:round}@keyframes spin{to{transform:rotate(360deg)}}@media(prefers-reduced-motion:reduce){*{animation-duration:0s!important;transition-duration:0s!important}}
"#
}

fn settings_css() -> &'static str {
    r#"
.settings-body{margin:0;overflow:hidden;background:var(--settings-bg);color:var(--text)}
.shortcut-control input[data-binding=""]+.clear-shortcut{visibility:hidden}
.input-editor{display:grid;gap:5px;min-width:0}.shortcut-control{display:flex;align-items:center;min-width:0;gap:4px}.shortcut-control input{min-width:0;width:100%;height:26px;padding:0 7px;border:1px solid transparent;border-radius:5px;background:transparent;color:var(--secondary);font-size:11px;text-overflow:ellipsis;cursor:pointer}.shortcut-control input:focus{outline:2px solid var(--accent);outline-offset:0;border-color:transparent;background:var(--input-bg);color:var(--text)}.clear-shortcut{flex:none;width:24px;height:24px;border-radius:4px;background:transparent;color:var(--secondary);font-size:17px;cursor:pointer}.clear-shortcut:hover{background:var(--button-pressed)}.previous-shortcut{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:14px 12px;font-size:13px}.previous-shortcut .shortcut-control{width:215px}.shortcut-error{color:var(--danger);font-size:12px;line-height:1.4;max-height:60px;overflow:auto;padding:8px 0}.actions .notice.failed{max-width:230px;max-height:60px;overflow:auto;overflow-wrap:anywhere}.settings-header{flex-wrap:wrap}.monitor-tabs{max-width:100%;overflow:auto}.settings-page .column-heading,.settings-page .input-row{grid-template-columns:24px minmax(84px,1fr) minmax(140px,1.35fr) 36px;gap:8px}
.settings-page{height:100vh;display:flex;flex-direction:column;padding:24px 28px 18px}
.display-panel .previous-shortcut{margin:0 2px 18px;padding:12px 0 16px;border-bottom:1px solid var(--separator)}.previous-shortcut>span:first-child{font-weight:600;color:var(--text)}.previous-shortcut .shortcut-control{flex:0 1 215px}.previous-shortcut input{height:32px;border-color:var(--input-border);background:var(--input-bg);color:var(--text);font-size:12px;text-align:center}
.settings-header{display:flex;align-items:center;gap:20px;min-height:34px}.settings-header h1{margin:0;font-size:19px;line-height:1.25;font-weight:600}.monitor-tabs{display:flex;min-width:0;margin-left:auto;padding:2px;border:1px solid var(--segment-border);border-radius:7px;background:var(--segmented-bg)}.monitor-tab{height:29px;max-width:180px;display:flex;align-items:center;justify-content:center;gap:6px;padding:0 12px;border-radius:5px;background:transparent;color:var(--secondary);font-size:12px;font-weight:500;white-space:nowrap;overflow:hidden;transition:background-color .12s ease-out,color .12s ease-out}.monitor-tab span{overflow:hidden;text-overflow:ellipsis}.monitor-tab svg{width:14px;height:14px;flex:0 0 auto}.monitor-tab.active{background:var(--segment-active);color:var(--text);box-shadow:var(--segment-shadow)}
.settings-scroll{min-height:0;margin:20px -6px 0;padding:0 6px 8px;overflow:auto}.display-panel{display:none}.display-panel.active{display:block}.display-heading{display:flex;align-items:center;gap:14px;margin:0 2px 11px}.display-heading>span:first-child{display:grid;gap:3px;min-width:0}.display-heading strong{font-size:14px;font-weight:600}.display-heading small{max-width:410px;color:var(--secondary);font-size:11px;line-height:1.35;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.display-state{display:flex;align-items:center;gap:6px;margin-left:auto;color:var(--secondary);font-size:11px;white-space:nowrap}.column-heading{display:grid;grid-template-columns:28px minmax(120px,1fr) minmax(210px,1.35fr) 44px;gap:11px;padding:0 13px 7px;color:var(--secondary);font-size:11px}.column-heading span:first-child{grid-column:2}.column-heading span:last-child{text-align:center}.input-list{border:1px solid var(--border);border-radius:8px;overflow:hidden;background:var(--group-bg);box-shadow:var(--group-shadow)}
.input-row{min-height:55px;display:grid;grid-template-columns:28px minmax(120px,1fr) minmax(210px,1.35fr) 44px;align-items:center;gap:11px;padding:8px 12px;border-bottom:1px solid var(--separator);transition:background-color .12s ease-out}.input-row:last-child{border-bottom:0}.input-row:focus-within{background:var(--row-focus)}.input-row.candidate .port-icon,.input-row.candidate .physical-source{opacity:.72}.port-icon{display:grid;place-items:center;color:var(--secondary)}.port-icon svg{width:17px;height:17px}.physical-source{display:grid;gap:2px;min-width:0}.physical-source strong{font-size:13px;font-weight:500}.physical-source small{color:var(--secondary);font-size:11px;font-weight:400;line-height:1.25;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.name-field{width:100%;height:30px;border:1px solid var(--input-border);border-radius:6px;background:var(--input-bg);color:var(--text);padding:0 9px;outline:none;font-size:13px;transition:border-color .12s ease-out,box-shadow .12s ease-out}.name-field:focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--focus)}.menu-toggle{display:grid;place-items:center;width:44px;height:36px;cursor:default}.menu-toggle input{position:absolute;opacity:0;pointer-events:none}.toggle-track{position:relative;width:30px;height:18px;border-radius:9px;background:var(--toggle-off);box-shadow:inset 0 0 0 1px var(--toggle-border);transition:background-color .14s ease-out}.toggle-track span{position:absolute;top:2px;left:2px;width:14px;height:14px;border-radius:50%;background:white;box-shadow:0 1px 2px rgba(0,0,0,.25);transition:transform .16s var(--ease)}.menu-toggle input:checked+.toggle-track{background:var(--accent);box-shadow:none}.menu-toggle input:checked+.toggle-track span{transform:translateX(12px)}.menu-toggle input:focus-visible+.toggle-track{outline:3px solid var(--focus);outline-offset:1px}
.settings-page footer{margin-top:auto;min-height:44px;display:flex;align-items:flex-end;justify-content:space-between;gap:20px;padding-top:12px}.backend-status{max-width:300px;color:var(--secondary);font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.actions{display:flex;align-items:center;gap:8px;flex:0 0 auto}.actions .notice{margin-right:3px;color:var(--success);font-size:12px}.actions .notice svg{width:14px;height:14px}.actions button{min-width:72px;height:31px;padding:0 15px;border:1px solid var(--input-border);border-radius:7px;background:var(--button-bg);font-size:13px;text-align:center;transition:background-color .12s ease-out,border-color .12s ease-out}.actions button:not(:disabled):hover{background:var(--button-hover)}.actions button:not(:disabled):active{background:var(--button-pressed)}.actions button.primary{border-color:transparent;background:var(--accent);color:white}.actions button.primary:not(:disabled):hover{background:var(--accent-hover)}.actions button:disabled{opacity:.42}
.actions .notice.failed{color:var(--danger)}.actions .notice.failed svg{display:none}dialog{width:min(360px,calc(100vw - 32px));padding:20px;border:1px solid var(--border);border-radius:10px;background:var(--group-bg);color:var(--text);box-shadow:0 14px 40px rgba(0,0,0,.22)}dialog::backdrop{background:rgba(0,0,0,.25)}dialog h2{margin:0 0 8px;font-size:16px}dialog p{margin:0 0 20px;color:var(--secondary);font-size:13px;line-height:1.45}.dialog-actions{display:flex;justify-content:flex-end;gap:8px}.dialog-actions button{padding:7px 12px;border:1px solid var(--input-border);border-radius:6px;background:var(--button-bg);cursor:pointer}.dialog-actions .destructive{border-color:transparent;background:var(--danger);color:#fff}.monitor-tab:focus-visible,.actions button:focus-visible,.dialog-actions button:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.macos{--settings-bg:#f2f2f4;--group-bg:#fff;--segmented-bg:rgba(0,0,0,.045);--segment-border:rgba(0,0,0,.10);--segment-active:#fff;--segment-shadow:0 1px 2px rgba(0,0,0,.13);--text:#1d1d1f;--secondary:#69696e;--border:rgba(0,0,0,.14);--separator:rgba(0,0,0,.085);--input-border:rgba(0,0,0,.2);--input-bg:#fff;--button-bg:#fff;--button-hover:#f8f8f8;--button-pressed:#efeff1;--row-focus:rgba(10,132,255,.055);--accent:#087ff5;--accent-hover:#0077e8;--focus:rgba(10,132,255,.18);--success:#238636;--danger:#c93434;--toggle-off:#b7b7ba;--toggle-border:rgba(0,0,0,.08);--group-shadow:0 1px 2px rgba(0,0,0,.035)}
.windows{--settings-bg:#f7f7f7;--group-bg:#fff;--segmented-bg:#ededed;--segment-border:rgba(0,0,0,.08);--segment-active:#fff;--segment-shadow:0 1px 2px rgba(0,0,0,.12);--text:#1b1b1b;--secondary:#5f5f5f;--border:rgba(0,0,0,.12);--separator:rgba(0,0,0,.08);--input-border:rgba(0,0,0,.19);--input-bg:#fff;--button-bg:#fff;--button-hover:#f5f5f5;--button-pressed:#ededed;--row-focus:rgba(0,103,192,.045);--accent:#0067c0;--accent-hover:#005a9e;--focus:rgba(0,103,192,.16);--success:#107c10;--danger:#c42b1c;--toggle-off:#b7b7b7;--toggle-border:rgba(0,0,0,.08);--group-shadow:none;font-family:"Segoe UI Variable Text","Segoe UI",sans-serif}.windows .settings-page{padding:24px 30px 20px}.windows .actions button{height:34px;border-radius:4px}
@media(prefers-color-scheme:dark){.macos{--settings-bg:#242426;--group-bg:#303033;--segmented-bg:rgba(255,255,255,.07);--segment-border:rgba(255,255,255,.10);--segment-active:rgba(255,255,255,.13);--segment-shadow:0 1px 2px rgba(0,0,0,.3);--text:#f5f5f7;--secondary:#b1b1b6;--border:rgba(255,255,255,.14);--separator:rgba(255,255,255,.09);--input-border:rgba(255,255,255,.18);--input-bg:#3a3a3d;--button-bg:#363639;--button-hover:#404043;--button-pressed:#48484c;--row-focus:rgba(10,132,255,.09);--accent:#0a84ff;--accent-hover:#2491ff;--success:#47c95d;--toggle-off:#59595c;--toggle-border:rgba(255,255,255,.1)}.windows{--settings-bg:#202020;--group-bg:#2b2b2b;--segmented-bg:#292929;--segment-border:rgba(255,255,255,.10);--segment-active:#3b3b3b;--segment-shadow:none;--text:#fff;--secondary:#c8c8c8;--border:rgba(255,255,255,.11);--separator:rgba(255,255,255,.09);--input-border:rgba(255,255,255,.16);--input-bg:#323232;--button-bg:#323232;--button-hover:#3a3a3a;--button-pressed:#414141;--row-focus:rgba(96,205,255,.07);--accent:#60cdff;--accent-hover:#76d5ff;--success:#6ccb5f;--toggle-off:#666;--toggle-border:rgba(255,255,255,.1)}.windows .actions button.primary{color:#111}}
"#
}

fn icons_js() -> &'static str {
    r#"{
      "monitor":"<svg viewBox='0 0 24 24'><rect x='3' y='4' width='18' height='12' rx='2'/><path d='M8 20h8M12 16v4'/></svg>",
      "settings":"<svg viewBox='0 0 24 24'><circle cx='12' cy='12' r='3'/><path d='M19 13.5v-3l-2.1-.7-.5-1.2 1-2-2.1-2.1-2 1-1.2-.5-.7-2.1h-3L7.7 5l-1.2.5-2-1-2.1 2.1 1 2-.5 1.2-2.1.7v3l2.1.7.5 1.2-1 2 2.1 2.1 2-1 1.2.5.7 2.1h3l.7-2.1 1.2-.5 2 1 2.1-2.1-1-2 .5-1.2 2.1-.7Z'/></svg>",
      "quit":"<svg viewBox='0 0 24 24'><path d='M10 5H5v14h5M14 8l4 4-4 4M8 12h10'/></svg>",
      "warning":"<svg viewBox='0 0 24 24'><path d='M12 3 2.8 20h18.4L12 3Z'/><path d='M12 9v5M12 17h.01'/></svg>",
      "checkCircle":"<svg viewBox='0 0 24 24'><circle cx='12' cy='12' r='9'/><path d='m8 12 2.5 2.5L16 9'/></svg>",
      "usb-c":"<svg viewBox='0 0 24 24'><rect x='4' y='8' width='16' height='8' rx='4'/><path d='M8 12h8'/></svg>",
      "hdmi":"<svg viewBox='0 0 24 24'><path d='M3 8h18l-2 8H5L3 8Z'/><path d='M8 16v2h8v-2'/></svg>",
      "displayport":"<svg viewBox='0 0 24 24'><path d='M5 4h10a6 6 0 0 1 0 12h-5v4H5V4Z'/><path d='M10 9h5a1 1 0 0 1 0 2h-5V9Z'/></svg>"
    }"#
}

#[cfg(test)]
mod tests {
    use super::settings_html;
    use crate::ui::{InputView, MonitorView, OperationView, ProfileView, UiState};

    fn state() -> UiState {
        let configured = InputView {
            index: 1,
            name: "Gaming PC".to_string(),
            subtitle: "HDMI 1".to_string(),
            value: 0x11,
            hex: "0x11".to_string(),
            profile_name: Some("pc".to_string()),
            display_selector: "display-1".to_string(),
            icon: "hdmi".to_string(),
            current: true,
            show_in_menu: true,
            reported_supported: true,
            shortcut: String::new(),
        };
        let candidate = InputView {
            index: 2,
            name: "HDMI 2".to_string(),
            subtitle: "Not configured".to_string(),
            value: 0x12,
            hex: "0x12".to_string(),
            profile_name: None,
            display_selector: "display-1".to_string(),
            icon: "hdmi".to_string(),
            current: false,
            show_in_menu: false,
            reported_supported: false,
            shortcut: String::new(),
        };
        let monitor = MonitorView {
            selector: "display-1".to_string(),
            name: "External Display".to_string(),
            subtitle: String::new(),
            current_value: Some(0x11),
            current_hex: Some("0x11".to_string()),
            connected: true,
            inputs: vec![configured.clone()],
            available_inputs: vec![configured.clone(), candidate.clone()],
        };
        UiState {
            config_error: None,
            shortcuts: Vec::new(),
            shortcut_error: None,
            previous_inputs: std::collections::HashMap::new(),
            monitor: monitor.clone(),
            monitors: vec![monitor],
            inputs: vec![configured.clone()],
            available_inputs: vec![configured, candidate],
            profiles: vec![ProfileView {
                name: "pc".to_string(),
                label: "Gaming PC".to_string(),
                value: 0x11,
                hex: "0x11".to_string(),
                display: "-".to_string(),
                backend: "auto".to_string(),
            }],
            config_path: "/tmp/config.toml".to_string(),
            backend: "auto".to_string(),
            operation: OperationView::default(),
        }
    }

    #[test]
    fn settings_uses_available_candidates_without_advanced_fields() {
        let html = settings_html(&state());
        assert!(html.contains("HDMI 2"));
        assert!(html.contains("saveLabels"));
        assert!(html.contains("data-visibility"));
        assert!(html.contains("通用候选"));
        assert!(html.contains("structureKey"));
        assert!(html.contains("updateLiveState"));
        assert!(!html.contains("配置 ID"));
        assert!(html.contains("菜单名称</span>"));
        assert!(!html.contains("animation:panel-in"));
    }
}
