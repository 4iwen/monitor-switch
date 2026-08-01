use crate::ui::UiState;

pub fn popover_html(state: &UiState) -> String {
    let state_json = serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string());
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <style>{base}{popover}</style>
</head>
<body class="{platform} popover-body">
  <main id="app"></main>
  <script>
    const ICONS = {icons};
    const PLATFORM = "{platform}";
    const send = (type, payload = {{}}) => window.ipc.postMessage(JSON.stringify({{ type, ...payload }}));
    const icon = name => ICONS[name] || ICONS.monitor;
    const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, char => ({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[char]));

    function render(state) {{
      window.__STATE__ = state;
      const operation = state.operation || {{ status: 'idle' }};
      const inputs = state.inputs.map(input => {{
        const switchingActive = operation.status === 'switching';
        const switching = operation.status === 'switching' && operation.target_value === input.value;
        const selected = switching || (!switchingActive && input.current);
        return `
          <button class="source-row ${{selected ? 'selected' : ''}} ${{switching ? 'switching' : ''}}"
                  data-switch-value="${{input.value}}"
                  data-display="${{escapeHtml(input.display_selector)}}"
                  data-profile="${{escapeHtml(input.profile_name || '')}}"
                  ${{operation.status === 'switching' ? 'disabled' : ''}}>
            <span class="source-icon">${{icon(input.icon)}}</span>
            <span class="source-copy">
              <strong>${{escapeHtml(input.name)}}</strong>
              <small>${{escapeHtml(input.subtitle === 'Not configured' ? input.hex : input.subtitle)}}</small>
            </span>
            ${{selected ? `<span class="check" aria-label="${{switching ? '正在切换' : '当前输入源'}}">✓</span>` : ''}}
          </button>`;
      }}).join('');

      const error = operation.status === 'error'
        ? `<div class="error-banner" role="alert">${{icon('warning')}}<span>${{escapeHtml(operation.message || '切换失败，请重试')}}</span></div>`
        : '';

      document.getElementById('app').innerHTML = `
        <section class="popover-surface">
          <header class="popover-header">
            <div>
              <h1>Monitor Switch</h1>
              <p><i class="connection ${{state.monitor.connected ? 'online' : ''}}"></i>${{escapeHtml(state.monitor.name)}} · ${{state.monitor.connected ? 'DDC/CI 已连接' : '无法读取当前输入源'}}</p>
            </div>
          </header>
          <div class="separator"></div>
          <section class="sources" aria-label="输入源">
            <h2>输入源</h2>
            ${{inputs || '<div class="empty">尚未检测到输入源</div>'}}
          </section>
          ${{error}}
          <div class="separator footer-separator"></div>
          <footer class="popover-footer">
            <button onclick="send('openSettings')">${{icon('settings')}}<span>设置…</span></button>
            <button onclick="send('quit')">${{icon('quit')}}<span>退出${{PLATFORM === 'macos' ? ' Monitor Switch' : ''}}</span></button>
          </footer>
        </section>`;
      document.querySelectorAll('[data-switch-value]').forEach(element => {{
        element.addEventListener('click', () => switchInput(element));
      }});
    }}

    function switchInput(element) {{
      if (element.disabled) return;
      const profile = element.dataset.profile || null;
      send('switchInput', {{ value: Number(element.dataset.switchValue), display: element.dataset.display || null, profile }});
    }}

    window.monitorSwitchRender = render;
    render({state_json});
  </script>
</body>
</html>"#,
        base = base_css(),
        popover = popover_css(),
        icons = icons_js(),
        platform = platform_name(),
        state_json = state_json,
    )
}

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

    function render(state) {{
      window.__STATE__ = state;
      const displays = state.monitors.map((monitor, monitorIndex) => {{
        const rows = monitor.available_inputs.map(input => `
            <label class="rename-row">
              <span class="port-icon">${{icon(input.icon)}}</span>
              <span class="physical-source">${{escapeHtml(input.subtitle)}}<small>${{escapeHtml(input.hex)}}</small></span>
              <input data-value="${{input.value}}" data-display="${{escapeHtml(monitor.selector)}}" data-display-index="${{monitorIndex}}" data-default="${{escapeHtml(input.name)}}" value="${{escapeHtml(input.name)}}" maxlength="48" aria-label="${{escapeHtml(input.subtitle)}} 的显示名称">
            </label>`).join('');
        return `
          <section class="display-group" aria-label="${{escapeHtml(monitor.name)}}">
            <header class="display-heading">
              <span class="display-symbol">${{icon('monitor')}}</span>
              <span><strong>${{escapeHtml(monitor.name)}}</strong><small>${{escapeHtml(monitor.subtitle)}}</small></span>
              <span class="display-state"><i class="connection ${{monitor.connected ? 'online' : ''}}"></i>${{monitor.connected ? '已连接' : '未响应'}}</span>
            </header>
            <div class="column-heading"><span>物理输入源</span><span>菜单显示名称</span></div>
            <div class="rename-list">${{rows}}</div>
          </section>`;
      }}).join('');

      document.getElementById('app').innerHTML = `
        <section class="settings-page">
          <header class="settings-header">
            <h1>输入源名称</h1>
            <p>为菜单中的输入源设置容易识别的名称。</p>
          </header>
          <div class="settings-scroll">${{displays || '<div class="empty">尚未检测到可配置的显示器</div>'}}</div>
          <footer>
            <span class="backend-status"><i class="connection ${{state.monitor.connected ? 'online' : ''}}"></i>通过 ${{escapeHtml(state.backend)}} 控制 ${{escapeHtml(state.monitor.name)}}</span>
            <div class="actions">
              <div id="notice" class="notice" role="status">${{icon('checkCircle')}}<span>已保存</span></div>
              <button onclick="send('closeSettings')">取消</button>
              <button id="save-button" class="primary" onclick="saveLabels()" disabled>保存</button>
            </div>
          </footer>
        </section>`;
    }}

    function saveLabels() {{
      const items = [...document.querySelectorAll('input[data-value]')]
        .filter(input => input.value.trim() !== input.dataset.default)
        .map(input => ({{ value: Number(input.dataset.value), display: input.dataset.display || '', display_index: Number(input.dataset.displayIndex), label: input.value.trim() }}));
      send('saveLabels', {{ items }});
    }}

    function updateDirtyState() {{
      const dirty = [...document.querySelectorAll('input[data-value]')]
        .some(input => input.value.trim() !== input.dataset.default);
      const button = document.getElementById('save-button');
      if (button) button.disabled = !dirty;
    }}

    window.monitorSwitchRender = render;
    window.monitorSwitchSaved = () => {{
      const notice = document.getElementById('notice');
      if (!notice) return;
      notice.classList.add('visible');
      document.querySelectorAll('input[data-value]').forEach(input => input.dataset.default = input.value.trim());
      updateDirtyState();
      setTimeout(() => notice.classList.remove('visible'), 2400);
    }};
    window.monitorSwitchSaveError = message => alert(message);
    window.addEventListener('keydown', event => {{ if (event.key === 'Escape') send('closeSettings'); }});
    document.addEventListener('input', event => {{ if (event.target.matches('input[data-value]')) updateDirtyState(); }});
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
*{box-sizing:border-box}html,body,#app{margin:0;width:100%;height:100%}body{font-family:-apple-system,BlinkMacSystemFont,"SF Pro Text","Segoe UI",sans-serif;letter-spacing:0;-webkit-font-smoothing:antialiased}button,input{font:inherit}button{border:0;color:inherit}.separator{height:1px}.connection{display:inline-block;width:8px;height:8px;border-radius:50%;background:#9b9b9f}.connection.online{background:#34c759}.empty{padding:24px;text-align:center;color:var(--secondary);font-size:13px}.spinner{width:17px;height:17px;border-radius:50%;border:2px solid rgba(100,100,105,.22);border-top-color:var(--accent);animation:spin .72s linear infinite}.check{font-size:18px;font-weight:700}.error-banner{display:flex;align-items:flex-start;gap:8px;margin:8px 12px 0;padding:9px 10px;border-radius:8px;font-size:12px;line-height:1.35}.error-banner svg{width:16px;height:16px;flex:0 0 auto}.notice{display:flex;align-items:center;gap:8px;opacity:0;transform:translateY(5px);pointer-events:none;transition:opacity .18s var(--ease),transform .18s var(--ease)}.notice.visible{opacity:1;transform:none}svg{width:19px;height:19px;stroke:currentColor;fill:none;stroke-width:1.8;stroke-linecap:round;stroke-linejoin:round}@keyframes spin{to{transform:rotate(360deg)}}@keyframes appear{from{opacity:0;transform:translateY(-6px) scale(.985)}to{opacity:1;transform:none}}@media(prefers-reduced-motion:reduce){*{animation-duration:0s!important;transition-duration:0s!important}}
"#
}

fn popover_css() -> &'static str {
    r#"
.popover-body{overflow:hidden;background:transparent;padding:0;color:var(--text)}
.popover-surface{position:relative;height:100%;display:flex;flex-direction:column;overflow:hidden;background:var(--surface);border:1px solid var(--border);box-shadow:var(--shadow)}
.popover-header{display:flex;align-items:center;flex:0 0 auto;padding:14px 15px 10px}.popover-header h1{margin:0 0 3px;font-size:14px;line-height:1.2;font-weight:650}.popover-header p{display:flex;align-items:center;gap:6px;margin:0;color:var(--secondary);font-size:11px}
.separator{height:1px;flex:0 0 auto;background:var(--separator);margin:0 11px}.sources{min-height:0;flex:1;padding:8px 7px 1px;overflow:auto}.sources h2{margin:0;padding:0 7px 4px;color:var(--secondary);font-size:10px;font-weight:500}
.source-row{position:relative;width:100%;min-height:44px;display:flex;align-items:center;gap:9px;padding:5px 9px;border-radius:var(--row-radius);background:transparent;text-align:left;cursor:default;transition:background .12s ease}.source-row:not(:disabled):hover{background:var(--hover)}.source-row.selected{background:var(--selected);color:var(--selected-text)}.source-row:disabled{opacity:1}
.source-icon{display:grid;place-items:center;width:25px;height:25px;flex:0 0 auto}.source-icon svg{width:17px;height:17px}.source-copy{display:grid;gap:1px;min-width:0;flex:1}.source-copy strong{font-size:13px;font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.source-copy small{font-size:10px;color:var(--secondary)}.source-row.selected .source-copy small{color:var(--selected-secondary)}.check{color:inherit;font-size:16px}
.footer-separator{margin-top:6px}.popover-footer{display:flex;flex:0 0 auto;padding:5px 7px 7px;gap:3px}.popover-footer button{min-height:30px;display:flex;align-items:center;gap:7px;padding:0 8px;border-radius:var(--row-radius);background:transparent;font-size:12px}.popover-footer button:hover{background:var(--hover)}.popover-footer button:last-child{margin-left:auto}.popover-footer svg{width:16px;height:16px}.error-banner{max-height:62px;overflow:auto;flex:0 0 auto;color:#b42318;background:rgba(255,59,48,.10)}
.macos{--surface:transparent;--text:#202022;--secondary:#6f6f74;--separator:rgba(0,0,0,.105);--hover:rgba(0,0,0,.055);--selected:#0a84ff;--selected-text:white;--selected-secondary:rgba(255,255,255,.82);--accent:#0a84ff;--row-radius:6px}.macos .popover-surface{border:0;box-shadow:none;border-radius:18px}.macos .sources{scrollbar-width:none}.macos .sources::-webkit-scrollbar{display:none}.macos .popover-footer{display:block}.macos .popover-footer button{width:100%}.macos .popover-footer button:last-child{margin-left:0}
.windows{--surface:rgba(248,248,248,.96);--text:#1b1b1b;--secondary:#616161;--border:rgba(0,0,0,.12);--separator:rgba(0,0,0,.09);--hover:rgba(0,0,0,.045);--selected:rgba(0,95,184,.09);--selected-text:#1b1b1b;--selected-secondary:#616161;--accent:#0067c0;--row-radius:4px;--shadow:0 12px 30px rgba(0,0,0,.18),0 2px 8px rgba(0,0,0,.10);font-family:"Segoe UI Variable","Segoe UI",sans-serif}.windows.popover-body{padding:8px}.windows .popover-surface{border-radius:8px;backdrop-filter:blur(24px) saturate(130%)}.windows .popover-header{padding:17px 16px 11px}.windows .popover-header h1{font-size:15px}.windows .popover-header p{font-size:12px}.windows .sources{padding:9px 8px 2px}.windows .sources h2{padding:0 8px 5px;font-size:11px}.windows .source-row{min-height:56px;gap:11px;padding:6px 10px}.windows .source-row.selected:before{content:"";position:absolute;left:2px;top:10px;bottom:10px;width:3px;border-radius:2px;background:var(--accent)}.windows .source-icon{width:28px;height:28px}.windows .source-icon svg{width:19px;height:19px}.windows .source-copy{gap:2px}.windows .source-copy strong{font-size:14px}.windows .source-copy small{font-size:11px}.windows .popover-footer{padding:6px 8px 8px}.windows .popover-footer button{min-width:90px;min-height:34px;justify-content:center;font-size:13px}.windows .popover-footer button:last-child{margin-left:auto}
@media(prefers-color-scheme:dark){.macos{--surface:transparent;--text:#f5f5f7;--secondary:#aaaab0;--separator:rgba(255,255,255,.105);--hover:rgba(255,255,255,.075)}.windows{--surface:rgba(42,42,42,.97);--text:#fff;--secondary:#c8c8c8;--border:rgba(255,255,255,.11);--separator:rgba(255,255,255,.10);--hover:rgba(255,255,255,.07);--selected:rgba(96,205,255,.12);--selected-text:#fff;--selected-secondary:#c8c8c8;--accent:#60cdff}}
"#
}

fn settings_css() -> &'static str {
    r#"
.settings-body{margin:0;overflow:hidden;background:var(--settings-bg);color:var(--text)}
.settings-page{height:100vh;display:flex;flex-direction:column;padding:28px 30px 20px}
.settings-header h1{margin:0 0 5px;font-size:21px;line-height:1.2;font-weight:650}.settings-header p{margin:0;color:var(--secondary);font-size:13px;line-height:1.35}
.settings-scroll{min-height:0;margin:22px -8px 0;padding:0 8px 8px;overflow:auto}.display-group{margin-bottom:22px}.display-heading{display:flex;align-items:center;gap:10px;margin:0 2px 9px}.display-symbol{display:grid;place-items:center;width:30px;height:30px;border-radius:7px;background:var(--symbol-bg);color:var(--secondary)}.display-symbol svg{width:17px;height:17px}.display-heading>span:nth-child(2){display:grid;gap:1px;min-width:0}.display-heading strong{font-size:13px;font-weight:600}.display-heading small{max-width:420px;color:var(--secondary);font-size:10px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.display-state{display:flex;align-items:center;gap:5px;margin-left:auto;color:var(--secondary);font-size:10px}.column-heading{display:grid;grid-template-columns:minmax(132px,1fr) minmax(250px,1.35fr);gap:18px;padding:0 15px 6px 54px;color:var(--secondary);font-size:10px}.rename-list{border:1px solid var(--border);border-radius:10px;overflow:hidden;background:var(--group-bg);box-shadow:var(--group-shadow)}
.rename-row{min-height:58px;display:grid;grid-template-columns:28px minmax(118px,1fr) minmax(230px,1.25fr);align-items:center;gap:12px;padding:9px 14px;border-bottom:1px solid var(--separator)}.rename-row:last-child{border-bottom:0}.rename-row:focus-within{background:var(--row-focus)}
.port-icon{display:grid;place-items:center;color:var(--secondary)}.port-icon svg{width:18px;height:18px}.physical-source{display:grid;gap:2px;min-width:0;font-size:13px;font-weight:500}.physical-source small{color:var(--secondary);font-size:10px;font-weight:400}
.rename-row input{width:100%;height:30px;border:1px solid var(--input-border);border-radius:6px;background:var(--input-bg);color:var(--text);padding:0 9px;outline:none;transition:border-color .15s ease,box-shadow .15s ease}.rename-row input:focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--focus)}
.settings-page footer{margin-top:auto;min-height:48px;display:flex;align-items:flex-end;justify-content:space-between;gap:20px;padding-top:16px}.backend-status{display:flex;align-items:center;gap:7px;max-width:330px;color:var(--secondary);font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.actions{display:flex;align-items:center;gap:9px;flex:0 0 auto}.actions .notice{margin-right:3px;color:var(--success);font-size:12px}.actions .notice svg{width:15px;height:15px}.actions button{min-width:76px;height:32px;padding:0 15px;border:1px solid var(--input-border);border-radius:7px;background:var(--button-bg);text-align:center;transition:filter .14s ease,transform .1s ease}.actions button:not(:disabled):hover{filter:brightness(.97)}.actions button:not(:disabled):active{transform:scale(.98)}.actions button.primary{border-color:transparent;background:var(--accent);color:white}.actions button:disabled{opacity:.42}
.macos{--settings-bg:#ececec;--group-bg:rgba(255,255,255,.78);--symbol-bg:rgba(0,0,0,.055);--text:#1f1f21;--secondary:#707074;--border:rgba(0,0,0,.13);--separator:rgba(0,0,0,.085);--input-border:rgba(0,0,0,.18);--input-bg:rgba(255,255,255,.92);--button-bg:rgba(255,255,255,.78);--row-focus:rgba(10,132,255,.045);--accent:#0a84ff;--focus:rgba(10,132,255,.17);--success:#238636;--group-shadow:0 1px 2px rgba(0,0,0,.035)}
.windows{--settings-bg:#f3f3f3;--group-bg:#fbfbfb;--symbol-bg:rgba(0,0,0,.055);--text:#1b1b1b;--secondary:#616161;--border:rgba(0,0,0,.10);--separator:rgba(0,0,0,.08);--input-border:rgba(0,0,0,.18);--input-bg:white;--button-bg:#fbfbfb;--row-focus:rgba(0,103,192,.04);--accent:#0067c0;--focus:rgba(0,103,192,.16);--success:#107c10;--group-shadow:none;font-family:"Segoe UI Variable","Segoe UI",sans-serif}.windows .settings-page{padding:30px 34px 24px}.windows .settings-header h1{font-size:26px}.windows .rename-row{min-height:62px}.windows .actions button{height:36px;border-radius:4px}
@media(prefers-color-scheme:dark){.macos{--settings-bg:#242424;--group-bg:rgba(255,255,255,.065);--symbol-bg:rgba(255,255,255,.08);--text:#f5f5f7;--secondary:#aaaab0;--border:rgba(255,255,255,.13);--separator:rgba(255,255,255,.09);--input-border:rgba(255,255,255,.17);--input-bg:rgba(255,255,255,.075);--button-bg:rgba(255,255,255,.09);--row-focus:rgba(10,132,255,.08);--success:#47c95d;--group-shadow:0 1px 2px rgba(0,0,0,.25)}.windows{--settings-bg:#202020;--group-bg:#2b2b2b;--symbol-bg:rgba(255,255,255,.08);--text:white;--secondary:#c8c8c8;--border:rgba(255,255,255,.10);--separator:rgba(255,255,255,.09);--input-border:rgba(255,255,255,.15);--input-bg:#323232;--button-bg:#323232;--row-focus:rgba(96,205,255,.06);--accent:#60cdff;--success:#6ccb5f}.windows .actions button.primary{color:#111}}
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
    use super::{popover_html, settings_html};
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
    fn popover_contains_only_real_actions() {
        let html = popover_html(&state());
        assert!(html.contains("Gaming PC"));
        assert!(html.contains("openSettings"));
        assert!(html.contains("quit"));
        assert!(html.contains("data-switch-value"));
        assert!(!html.contains("onclick=\"switchInput"));
        assert!(!html.contains("检测当前源"));
        assert!(!html.contains("⌘"));
        assert!(!html.contains("top-tabs"));
    }

    #[test]
    fn settings_uses_available_candidates_without_advanced_fields() {
        let html = settings_html(&state());
        assert!(html.contains("HDMI 2"));
        assert!(html.contains("saveLabels"));
        assert!(!html.contains("配置 ID"));
        assert!(html.contains("物理输入源</span>"));
    }
}
