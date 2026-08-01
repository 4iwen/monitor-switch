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

    function render(state) {{
      window.__STATE__ = state;
      activeMonitor = Math.min(activeMonitor, Math.max(0, state.monitors.length - 1));
      const monitorTabs = state.monitors.map((monitor, monitorIndex) => `
        <button class="monitor-tab ${{monitorIndex === activeMonitor ? 'active' : ''}}" data-monitor-tab="${{monitorIndex}}">
          ${{icon('monitor')}}<span>${{escapeHtml(monitor.name)}}</span>
        </button>`).join('');
      const displays = state.monitors.map((monitor, monitorIndex) => {{
        const rows = monitor.available_inputs.map(input => `
            <div class="input-row ${{input.reported_supported ? '' : 'candidate'}}" data-input-row>
              <span class="port-icon">${{icon(input.icon)}}</span>
              <span class="physical-source">
                <strong>${{escapeHtml(input.subtitle)}}</strong>
                <small>${{escapeHtml(input.hex)}} · ${{input.reported_supported ? '显示器报告支持' : '通用候选'}}</small>
              </span>
              <input class="name-field" data-value="${{input.value}}" data-display="${{escapeHtml(monitor.selector)}}" data-display-index="${{monitorIndex}}" data-default="${{escapeHtml(input.name)}}" value="${{escapeHtml(input.name)}}" maxlength="48" aria-label="${{escapeHtml(input.subtitle)}} 的显示名称">
              <label class="menu-toggle" title="显示在状态栏菜单">
                <input type="checkbox" data-visibility data-default-visible="${{input.show_in_menu}}" ${{input.show_in_menu ? 'checked' : ''}} aria-label="在菜单中显示 ${{escapeHtml(input.subtitle)}}">
                <span class="toggle-track"><span></span></span>
              </label>
            </div>`).join('');
        return `
          <section class="display-panel ${{monitorIndex === activeMonitor ? 'active' : ''}}" data-monitor-panel="${{monitorIndex}}" aria-label="${{escapeHtml(monitor.name)}}">
            <header class="display-heading">
              <span><strong>${{escapeHtml(monitor.name)}}</strong><small>${{escapeHtml(monitor.subtitle)}}</small></span>
              <span class="display-state"><i class="connection ${{monitor.connected ? 'online' : ''}}"></i>${{monitor.connected ? 'DDC/CI 已连接' : 'DDC/CI 未响应'}}</span>
            </header>
            <div class="column-heading"><span>输入源</span><span>菜单名称</span><span>显示</span></div>
            <div class="input-list">${{rows}}</div>
          </section>`;
      }}).join('');

      document.getElementById('app').innerHTML = `
        <section class="settings-page">
          <header class="settings-header">
            <h1>输入源</h1>
            <div class="monitor-tabs" role="tablist">${{monitorTabs}}</div>
          </header>
          <div class="settings-scroll">${{displays || '<div class="empty">尚未检测到可配置的显示器</div>'}}</div>
          <footer>
            <span class="backend-status">${{escapeHtml(state.backend)}}</span>
            <div class="actions">
              <div id="notice" class="notice" role="status">${{icon('checkCircle')}}<span>已保存</span></div>
              <button onclick="send('closeSettings')">取消</button>
              <button id="save-button" class="primary" onclick="saveLabels()" disabled>保存</button>
            </div>
          </footer>
        </section>`;
      document.querySelectorAll('[data-monitor-tab]').forEach(button => button.addEventListener('click', () => selectMonitor(Number(button.dataset.monitorTab))));
    }}

    function selectMonitor(index) {{
      activeMonitor = index;
      document.querySelectorAll('[data-monitor-tab]').forEach(tab => tab.classList.toggle('active', Number(tab.dataset.monitorTab) === index));
      document.querySelectorAll('[data-monitor-panel]').forEach(panel => panel.classList.toggle('active', Number(panel.dataset.monitorPanel) === index));
    }}

    function saveLabels() {{
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
      send('saveLabels', {{ items }});
    }}

    function updateDirtyState() {{
      const dirty = [...document.querySelectorAll('[data-input-row]')].some(row => {{
        const name = row.querySelector('[data-value]');
        const visibility = row.querySelector('[data-visibility]');
        return name.value.trim() !== name.dataset.default || String(visibility.checked) !== visibility.dataset.defaultVisible;
      }});
      const button = document.getElementById('save-button');
      if (button) button.disabled = !dirty;
    }}

    window.monitorSwitchRender = render;
    window.monitorSwitchSaved = () => {{
      const notice = document.getElementById('notice');
      if (!notice) return;
      notice.classList.add('visible');
      document.querySelectorAll('input[data-value]').forEach(input => input.dataset.default = input.value.trim());
      document.querySelectorAll('[data-visibility]').forEach(input => input.dataset.defaultVisible = String(input.checked));
      updateDirtyState();
      setTimeout(() => notice.classList.remove('visible'), 2400);
    }};
    window.monitorSwitchSaveError = message => alert(message);
    window.addEventListener('keydown', event => {{ if (event.key === 'Escape') send('closeSettings'); }});
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
*{box-sizing:border-box}html,body,#app{margin:0;width:100%;height:100%}body{font-family:-apple-system,BlinkMacSystemFont,"SF Pro Text","Segoe UI",sans-serif;letter-spacing:0;-webkit-font-smoothing:antialiased}button,input{font:inherit}button{border:0;color:inherit}.separator{height:1px}.connection{display:inline-block;width:8px;height:8px;border-radius:50%;background:#9b9b9f}.connection.online{background:#34c759}.empty{padding:24px;text-align:center;color:var(--secondary);font-size:13px}.spinner{width:17px;height:17px;border-radius:50%;border:2px solid rgba(100,100,105,.22);border-top-color:var(--accent);animation:spin .72s linear infinite}.check{font-size:18px;font-weight:700}.error-banner{display:flex;align-items:flex-start;gap:8px;margin:8px 12px 0;padding:9px 10px;border-radius:8px;font-size:12px;line-height:1.35}.error-banner svg{width:16px;height:16px;flex:0 0 auto}.notice{display:flex;align-items:center;gap:8px;opacity:0;transform:translateY(5px);pointer-events:none;transition:opacity .18s var(--ease),transform .18s var(--ease)}.notice.visible{opacity:1;transform:none}svg{width:19px;height:19px;stroke:currentColor;fill:none;stroke-width:1.8;stroke-linecap:round;stroke-linejoin:round}@keyframes spin{to{transform:rotate(360deg)}}@keyframes appear{from{opacity:0;transform:translateY(-6px) scale(.985)}to{opacity:1;transform:none}}@media(prefers-reduced-motion:reduce){*{animation-duration:0s!important;transition-duration:0s!important}}
"#
}

fn settings_css() -> &'static str {
    r#"
.settings-body{margin:0;overflow:hidden;background:var(--settings-bg);color:var(--text)}
.settings-page{height:100vh;display:flex;flex-direction:column;padding:22px 26px 18px}
.settings-header{display:flex;align-items:center;gap:20px;min-height:34px}.settings-header h1{margin:0;font-size:20px;line-height:1.2;font-weight:650}.monitor-tabs{display:flex;min-width:0;margin-left:auto;padding:2px;border-radius:8px;background:var(--segmented-bg)}.monitor-tab{height:28px;max-width:180px;display:flex;align-items:center;justify-content:center;gap:6px;padding:0 11px;border-radius:6px;background:transparent;color:var(--secondary);font-size:12px;white-space:nowrap;overflow:hidden}.monitor-tab span{overflow:hidden;text-overflow:ellipsis}.monitor-tab svg{width:14px;height:14px;flex:0 0 auto}.monitor-tab.active{background:var(--segment-active);color:var(--text);box-shadow:var(--segment-shadow)}
.settings-scroll{min-height:0;margin:18px -6px 0;padding:0 6px 8px;overflow:auto}.display-panel{display:none;animation:panel-in .16s var(--ease)}.display-panel.active{display:block}.display-heading{display:flex;align-items:center;gap:14px;margin:0 2px 10px}.display-heading>span:first-child{display:grid;gap:2px;min-width:0}.display-heading strong{font-size:13px;font-weight:600}.display-heading small{max-width:410px;color:var(--secondary);font-size:10px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.display-state{display:flex;align-items:center;gap:5px;margin-left:auto;color:var(--secondary);font-size:10px;white-space:nowrap}.column-heading{display:grid;grid-template-columns:28px minmax(120px,1fr) minmax(210px,1.35fr) 44px;gap:11px;padding:0 13px 6px;color:var(--secondary);font-size:10px}.column-heading span:first-child{grid-column:2}.column-heading span:last-child{text-align:center}.input-list{border:1px solid var(--border);border-radius:9px;overflow:hidden;background:var(--group-bg);box-shadow:var(--group-shadow)}
.input-row{min-height:52px;display:grid;grid-template-columns:28px minmax(120px,1fr) minmax(210px,1.35fr) 44px;align-items:center;gap:11px;padding:7px 12px;border-bottom:1px solid var(--separator);transition:background .14s ease}.input-row:last-child{border-bottom:0}.input-row:focus-within{background:var(--row-focus)}.input-row.candidate .port-icon,.input-row.candidate .physical-source{opacity:.68}.port-icon{display:grid;place-items:center;color:var(--secondary)}.port-icon svg{width:17px;height:17px}.physical-source{display:grid;gap:2px;min-width:0}.physical-source strong{font-size:12px;font-weight:500}.physical-source small{color:var(--secondary);font-size:9px;font-weight:400;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.name-field{width:100%;height:28px;border:1px solid var(--input-border);border-radius:6px;background:var(--input-bg);color:var(--text);padding:0 8px;outline:none;font-size:12px;transition:border-color .15s ease,box-shadow .15s ease}.name-field:focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--focus)}.menu-toggle{display:grid;place-items:center;width:44px;height:32px;cursor:default}.menu-toggle input{position:absolute;opacity:0;pointer-events:none}.toggle-track{position:relative;width:30px;height:18px;border-radius:9px;background:var(--toggle-off);box-shadow:inset 0 0 0 1px var(--toggle-border);transition:background .16s ease}.toggle-track span{position:absolute;top:2px;left:2px;width:14px;height:14px;border-radius:50%;background:white;box-shadow:0 1px 2px rgba(0,0,0,.28);transition:transform .18s var(--ease)}.menu-toggle input:checked+.toggle-track{background:var(--accent);box-shadow:none}.menu-toggle input:checked+.toggle-track span{transform:translateX(12px)}.menu-toggle input:focus-visible+.toggle-track{outline:3px solid var(--focus);outline-offset:1px}
.settings-page footer{margin-top:auto;min-height:44px;display:flex;align-items:flex-end;justify-content:space-between;gap:20px;padding-top:12px}.backend-status{max-width:300px;color:var(--secondary);font-size:10px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.actions{display:flex;align-items:center;gap:8px;flex:0 0 auto}.actions .notice{margin-right:3px;color:var(--success);font-size:11px}.actions .notice svg{width:14px;height:14px}.actions button{min-width:70px;height:30px;padding:0 14px;border:1px solid var(--input-border);border-radius:7px;background:var(--button-bg);font-size:12px;text-align:center;transition:filter .14s ease,transform .1s ease}.actions button:not(:disabled):hover{filter:brightness(.97)}.actions button:not(:disabled):active{transform:scale(.98)}.actions button.primary{border-color:transparent;background:var(--accent);color:white}.actions button:disabled{opacity:.42}
.macos{--settings-bg:#ececec;--group-bg:rgba(255,255,255,.82);--segmented-bg:rgba(0,0,0,.065);--segment-active:rgba(255,255,255,.92);--segment-shadow:0 1px 2px rgba(0,0,0,.16);--text:#202022;--secondary:#747478;--border:rgba(0,0,0,.13);--separator:rgba(0,0,0,.08);--input-border:rgba(0,0,0,.18);--input-bg:rgba(255,255,255,.94);--button-bg:rgba(255,255,255,.8);--row-focus:rgba(10,132,255,.045);--accent:#0a84ff;--focus:rgba(10,132,255,.18);--success:#238636;--toggle-off:#b7b7ba;--toggle-border:rgba(0,0,0,.08);--group-shadow:0 1px 2px rgba(0,0,0,.035)}
.windows{--settings-bg:#f3f3f3;--group-bg:#fbfbfb;--segmented-bg:rgba(0,0,0,.055);--segment-active:white;--segment-shadow:0 1px 2px rgba(0,0,0,.12);--text:#1b1b1b;--secondary:#616161;--border:rgba(0,0,0,.10);--separator:rgba(0,0,0,.08);--input-border:rgba(0,0,0,.18);--input-bg:white;--button-bg:#fbfbfb;--row-focus:rgba(0,103,192,.04);--accent:#0067c0;--focus:rgba(0,103,192,.16);--success:#107c10;--toggle-off:#b7b7b7;--toggle-border:rgba(0,0,0,.08);--group-shadow:none;font-family:"Segoe UI Variable","Segoe UI",sans-serif}.windows .settings-page{padding:24px 30px 20px}.windows .actions button{height:34px;border-radius:4px}
@keyframes panel-in{from{opacity:0;transform:translateY(3px)}to{opacity:1;transform:none}}
@media(prefers-color-scheme:dark){.macos{--settings-bg:#242424;--group-bg:rgba(255,255,255,.07);--segmented-bg:rgba(255,255,255,.08);--segment-active:rgba(255,255,255,.12);--segment-shadow:0 1px 2px rgba(0,0,0,.3);--text:#f5f5f7;--secondary:#aaaab0;--border:rgba(255,255,255,.13);--separator:rgba(255,255,255,.09);--input-border:rgba(255,255,255,.17);--input-bg:rgba(255,255,255,.075);--button-bg:rgba(255,255,255,.09);--row-focus:rgba(10,132,255,.08);--success:#47c95d;--toggle-off:#59595c;--toggle-border:rgba(255,255,255,.1)}.windows{--settings-bg:#202020;--group-bg:#2b2b2b;--segmented-bg:rgba(255,255,255,.08);--segment-active:rgba(255,255,255,.12);--segment-shadow:none;--text:white;--secondary:#c8c8c8;--border:rgba(255,255,255,.10);--separator:rgba(255,255,255,.09);--input-border:rgba(255,255,255,.15);--input-bg:#323232;--button-bg:#323232;--row-focus:rgba(96,205,255,.06);--accent:#60cdff;--success:#6ccb5f;--toggle-off:#666;--toggle-border:rgba(255,255,255,.1)}.windows .actions button.primary{color:#111}}
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
        assert!(!html.contains("配置 ID"));
        assert!(html.contains("菜单名称</span>"));
    }
}
