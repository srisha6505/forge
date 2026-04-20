const $ = (id) => document.getElementById(id);

async function jpost(url, body) {
  const r = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  return r.json();
}

async function loadModels() {
  const sel = $('model');
  try {
    const r = await fetch('/models').then(r => r.json());
    const all = r.data || r.models || [];
    console.log('raw models response:', all);
    const list = all.filter(m => {
      const caps = m.capabilities || {};
      if (caps.type && caps.type !== 'chat') return false;
      if (m.model_picker_enabled === false) return false;
      const policyState = m.policy && m.policy.state;
      if (policyState && policyState !== 'enabled') return false;
      return true;
    });
    if (!list.length) {
      sel.innerHTML = '<option value="">(no accessible chat models)</option>';
      return;
    }
    const seen = new Set();
    const unique = list.filter(m => { if (seen.has(m.id)) return false; seen.add(m.id); return true; });
    unique.sort((a, b) => (a.name || a.id).localeCompare(b.name || b.id));
    sel.innerHTML = unique.map(m => {
      const id = m.id;
      const vendor = m.vendor ? ` [${m.vendor}]` : '';
      const label = m.name ? `${m.name}${vendor}` : id;
      return `<option value="${id}">${label}</option>`;
    }).join('');
    const list2 = unique;
    const preferred = list2.find(m => /gpt-4o/i.test(m.id)) || list2.find(m => /sonnet/i.test(m.id));
    if (preferred) sel.value = preferred.id;
  } catch (e) {
    sel.innerHTML = `<option value="">(error: ${e.message})</option>`;
  }
}

function showChat() {
  $('auth').hidden = true;
  $('chat').hidden = false;
  $('msg-input').focus();
  loadModels();
}

function showAuth() {
  $('auth').hidden = false;
  $('chat').hidden = true;
  $('code-panel').hidden = true;
}

async function init() {
  const r = await fetch('/auth/status').then(r => r.json());
  if (r.authenticated) showChat();
}

$('login-btn').addEventListener('click', async () => {
  $('login-btn').disabled = true;
  const r = await jpost('/auth/start');
  $('user-code').textContent = r.user_code || '???';
  const a = $('verify-url');
  a.href = r.verification_uri || '#';
  a.textContent = r.verification_uri || '';
  $('code-panel').hidden = false;

  const copyBtn = async (btn, text) => {
    try { await navigator.clipboard.writeText(text); }
    catch { const ta = document.createElement('textarea'); ta.value = text; document.body.appendChild(ta); ta.select(); document.execCommand('copy'); ta.remove(); }
    const orig = btn.textContent;
    btn.textContent = 'Copied!';
    btn.classList.add('copied');
    setTimeout(() => { btn.textContent = orig; btn.classList.remove('copied'); }, 1200);
  };
  $('copy-url-btn').onclick = () => copyBtn($('copy-url-btn'), r.verification_uri);
  $('copy-code-btn').onclick = () => copyBtn($('copy-code-btn'), r.user_code);
  $('open-url-btn').onclick = () => window.open(r.verification_uri, '_blank', 'noopener');

  const delay = (r.interval || 5) * 1000;
  const tick = async () => {
    const p = await jpost('/auth/poll');
    $('poll-status').textContent = 'Status: ' + p.status;
    if (p.status === 'ok') {
      showChat();
      return;
    }
    if (p.status === 'expired_token' || p.status === 'access_denied') {
      $('login-btn').disabled = false;
      return;
    }
    setTimeout(tick, delay);
  };
  setTimeout(tick, delay);
});

$('logout-btn').addEventListener('click', async () => {
  await jpost('/auth/logout');
  $('messages').innerHTML = '';
  $('login-btn').disabled = false;
  showAuth();
});

$('send-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const input = $('msg-input');
  const msg = input.value.trim();
  if (!msg) return;
  input.value = '';
  addMsg('user', msg);
  const slot = addMsg('assistant', '…');
  const r = await jpost('/chat', { message: msg, model: $('model').value });
  if (r.reply) {
    slot.textContent = r.reply;
  } else {
    slot.className = 'msg error';
    const body = r.body ? '\n' + (typeof r.body === 'string' ? r.body : JSON.stringify(r.body, null, 2)) : '';
    slot.textContent = `Error (${r.error || 'unknown'})${body}`;
    console.error('chat error:', r);
  }
  $('messages').scrollTop = $('messages').scrollHeight;
});

function addMsg(role, text) {
  const el = document.createElement('div');
  el.className = 'msg ' + role;
  el.textContent = text;
  $('messages').appendChild(el);
  $('messages').scrollTop = $('messages').scrollHeight;
  return el;
}

init();
