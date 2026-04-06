// =============================================================================
// ShellWeGo Dashboard - Application Logic
// =============================================================================

const API_BASE = window.location.origin;
let currentSection = 'overview';
let refreshTimer = null;

// ── API Helper ──────────────────────────────────────────────────────────────

async function apiFetch(path, options = {}) {
    const token = localStorage.getItem('shellwego_token');
    const headers = { 'Content-Type': 'application/json', ...options.headers };
    if (token) headers['Authorization'] = `Bearer ${token}`;
    const res = await fetch(`${API_BASE}${path}`, { ...options, headers });
    if (res.status === 401) { logout(); throw new Error('Unauthorized'); }
    if (!res.ok) {
        const err = await res.json().catch(() => ({}));
        throw new Error(err.message || err.error || res.statusText);
    }
    return res.json();
}

// ── Authentication ──────────────────────────────────────────────────────────

async function login(email, password) {
    try {
        const data = await apiFetch('/v1/auth/login', {
            method: 'POST',
            body: JSON.stringify({ email, password })
        });
        if (data.access_token || data.token) {
            localStorage.setItem('shellwego_token', data.access_token || data.token);
            showDashboard();
            showToast('Signed in successfully', 'success');
        } else {
            throw new Error('No token received from server');
        }
    } catch (err) {
        const errorEl = document.getElementById('login-error');
        errorEl.textContent = err.message || 'Login failed';
        errorEl.hidden = false;
    }
}

function logout() {
    localStorage.removeItem('shellwego_token');
    if (refreshTimer) clearInterval(refreshTimer);
    showLogin();
    showToast('Signed out', 'success');
}

// ── Screen Management ───────────────────────────────────────────────────────

function showLogin() {
    document.getElementById('login-screen').classList.add('active');
    document.getElementById('login-screen').hidden = false;
    document.getElementById('dashboard-screen').classList.remove('active');
    document.getElementById('dashboard-screen').hidden = true;
    document.querySelector('.brand-footer').hidden = false;
}

function showDashboard() {
    document.getElementById('login-screen').classList.remove('active');
    document.getElementById('login-screen').hidden = true;
    document.getElementById('dashboard-screen').classList.add('active');
    document.getElementById('dashboard-screen').hidden = false;
    const token = localStorage.getItem('shellwego_token');
    if (token) {
        try {
            const payload = JSON.parse(atob(token.split('.')[1]));
            document.getElementById('user-email').textContent = payload.email || payload.sub || 'Admin';
        } catch {
            document.getElementById('user-email').textContent = 'Admin';
        }
    }
    loadSection('overview');
    // Auto-refresh overview every 15 seconds
    if (refreshTimer) clearInterval(refreshTimer);
    refreshTimer = setInterval(() => {
        if (currentSection === 'overview') loadSection('overview');
    }, 15000);
}

// ── Section Loading ─────────────────────────────────────────────────────────

async function loadSection(section) {
    currentSection = section;
    const main = document.getElementById('main-content');
    main.innerHTML = '<div class="loading">Loading...</div>';

    // Update sidebar active state
    document.querySelectorAll('#sidebar li').forEach(li => {
        li.classList.toggle('active', li.dataset.section === section);
    });

    try {
        switch (section) {
            case 'overview': await loadOverview(); break;
            case 'apps': await loadApps(); break;
            case 'nodes': await loadNodes(); break;
            case 'volumes': await loadVolumes(); break;
            case 'domains': await loadDomains(); break;
            case 'databases': await loadDatabases(); break;
            case 'secrets': await loadSecrets(); break;
            default: main.innerHTML = '<div class="card"><h3>Coming Soon</h3><p>This section is not yet implemented.</p></div>';
        }
    } catch (err) {
        main.innerHTML = `<div class="card"><h3>Error</h3><p>${escapeHtml(err.message)}</p><button class="btn btn-primary" onclick="loadSection('${section}')">Retry</button></div>`;
    }
}

// ── Overview ────────────────────────────────────────────────────────────────

async function loadOverview() {
    const [metrics, apps, nodes] = await Promise.all([
        apiFetch('/v1/metrics').catch(() => ({})),
        apiFetch('/v1/apps?limit=5').catch(() => ({ items: [] })),
        apiFetch('/v1/nodes').catch(() => ({ items: [] }))
    ]);

    const appsArray = apps.items || apps.data || apps || [];
    const nodesArray = nodes.items || nodes.data || nodes || [];
    const runningApps = Array.isArray(appsArray) ? appsArray.filter(a => a.status === 'running' || a.status === 'deployed').length : 0;
    const healthyNodes = Array.isArray(nodesArray) ? nodesArray.filter(n => n.status === 'online' || n.status === 'ready').length : 0;

    document.getElementById('main-content').innerHTML = `
        <h2 class="page-title">Overview</h2>
        <div class="stat-grid">
            <div class="card stat">
                <div class="value">${Array.isArray(appsArray) ? appsArray.length : 0}</div>
                <div class="label">Total Apps</div>
            </div>
            <div class="card stat">
                <div class="value">${runningApps}</div>
                <div class="label">Running Apps</div>
            </div>
            <div class="card stat">
                <div class="value">${Array.isArray(nodesArray) ? nodesArray.length : 0}</div>
                <div class="label">Total Nodes</div>
            </div>
            <div class="card stat">
                <div class="value">${healthyNodes}</div>
                <div class="label">Healthy Nodes</div>
            </div>
        </div>
        <div class="card">
            <h3>Recent Apps</h3>
            ${appsArray.length > 0 ? `
            <table>
                <thead><tr><th>Name</th><th>Status</th><th>Region</th></tr></thead>
                <tbody>
                    ${appsArray.slice(0, 5).map(app => `
                    <tr>
                        <td>${escapeHtml(app.name || app.id || 'unknown')}</td>
                        <td><span class="badge badge-${statusClass(app.status)}">${escapeHtml(app.status || 'unknown')}</span></td>
                        <td>${escapeHtml(app.region || 'default')}</td>
                    </tr>`).join('')}
                </tbody>
            </table>` : '<p class="muted">No apps deployed yet.</p>'}
        </div>
        <div class="card">
            <h3>Node Status</h3>
            ${nodesArray.length > 0 ? `
            <table>
                <thead><tr><th>Node</th><th>Status</th><th>CPU</th><th>Memory</th></tr></thead>
                <tbody>
                    ${nodesArray.slice(0, 10).map(node => `
                    <tr>
                        <td>${escapeHtml(node.name || node.id || 'unknown')}</td>
                        <td><span class="badge badge-${statusClass(node.status)}">${escapeHtml(node.status || 'unknown')}</span></td>
                        <td>${node.cpu_usage != null ? node.cpu_usage + '%' : 'N/A'}</td>
                        <td>${node.memory_usage != null ? node.memory_usage + '%' : 'N/A'}</td>
                    </tr>`).join('')}
                </tbody>
            </table>` : '<p class="muted">No nodes connected yet.</p>'}
        </div>
    `;
}

// ── Apps ────────────────────────────────────────────────────────────────────

async function loadApps() {
    const data = await apiFetch('/v1/apps');
    const apps = data.items || data.data || data || [];

    document.getElementById('main-content').innerHTML = `
        <div class="page-header">
            <h2 class="page-title">Apps</h2>
        </div>
        <div class="card">
            ${apps.length > 0 ? `
            <table>
                <thead><tr><th>Name</th><th>Status</th><th>Region</th><th>Created</th><th>Actions</th></tr></thead>
                <tbody>
                    ${Array.isArray(apps).map(app => `
                    <tr>
                        <td><strong>${escapeHtml(app.name || app.id)}</strong></td>
                        <td><span class="badge badge-${statusClass(app.status)}">${escapeHtml(app.status || 'unknown')}</span></td>
                        <td>${escapeHtml(app.region || 'default')}</td>
                        <td>${app.created_at ? new Date(app.created_at).toLocaleDateString() : 'N/A'}</td>
                        <td>
                            <button class="btn btn-primary btn-sm" onclick="deployApp('${escapeHtml(app.id)}')">Deploy</button>
                            <button class="btn btn-danger btn-sm" onclick="deleteApp('${escapeHtml(app.id)}')">Delete</button>
                        </td>
                    </tr>`).join('')}
                </tbody>
            </table>` : '<p class="muted">No apps found. Deploy your first app using the CLI or API.</p>'}
        </div>
    `;
}

async function deployApp(id) {
    try {
        await apiFetch(`/v1/apps/${id}/deploy`, { method: 'POST' });
        showToast('App deploy started', 'success');
        loadApps();
    } catch (err) {
        showToast(err.message, 'error');
    }
}

async function deleteApp(id) {
    if (!confirm('Are you sure you want to delete this app?')) return;
    try {
        await apiFetch(`/v1/apps/${id}`, { method: 'DELETE' });
        showToast('App deleted', 'success');
        loadApps();
    } catch (err) {
        showToast(err.message, 'error');
    }
}

// ── Nodes ───────────────────────────────────────────────────────────────────

async function loadNodes() {
    const data = await apiFetch('/v1/nodes');
    const nodes = data.items || data.data || data || [];

    document.getElementById('main-content').innerHTML = `
        <h2 class="page-title">Nodes</h2>
        <div class="card">
            ${nodes.length > 0 ? `
            <table>
                <thead><tr><th>Name</th><th>Status</th><th>Region</th><th>CPU Usage</th><th>Memory Usage</th><th>Disk Usage</th><th>Running VMs</th></tr></thead>
                <tbody>
                    ${Array.isArray(nodes).map(node => `
                    <tr>
                        <td><strong>${escapeHtml(node.name || node.id)}</strong></td>
                        <td><span class="badge badge-${statusClass(node.status)}">${escapeHtml(node.status || 'unknown')}</span></td>
                        <td>${escapeHtml(node.region || 'default')}</td>
                        <td>${node.cpu_usage != null ? node.cpu_usage + '%' : 'N/A'}</td>
                        <td>${node.memory_usage != null ? node.memory_usage + '%' : 'N/A'}</td>
                        <td>${node.disk_usage != null ? node.disk_usage + '%' : 'N/A'}</td>
                        <td>${node.running_vms != null ? node.running_vms : 'N/A'}</td>
                    </tr>`).join('')}
                </tbody>
            </table>` : '<p class="muted">No nodes connected. Start the agent to register nodes.</p>'}
        </div>
    `;
}

// ── Volumes ─────────────────────────────────────────────────────────────────

async function loadVolumes() {
    const data = await apiFetch('/v1/volumes').catch(() => ({ items: [] }));
    const volumes = data.items || data.data || data || [];

    document.getElementById('main-content').innerHTML = `
        <h2 class="page-title">Volumes</h2>
        <div class="card">
            ${volumes.length > 0 ? `
            <table>
                <thead><tr><th>Name</th><th>Size</th><th>Region</th><th>Status</th></tr></thead>
                <tbody>
                    ${Array.isArray(volumes).map(vol => `
                    <tr>
                        <td><strong>${escapeHtml(vol.name || vol.id)}</strong></td>
                        <td>${escapeHtml(vol.size || 'N/A')}</td>
                        <td>${escapeHtml(vol.region || 'default')}</td>
                        <td><span class="badge badge-${statusClass(vol.status)}">${escapeHtml(vol.status || 'unknown')}</span></td>
                    </tr>`).join('')}
                </tbody>
            </table>` : '<p class="muted">No volumes found.</p>'}
        </div>
    `;
}

// ── Domains ─────────────────────────────────────────────────────────────────

async function loadDomains() {
    const data = await apiFetch('/v1/domains').catch(() => ({ items: [] }));
    const domains = data.items || data.data || data || [];

    document.getElementById('main-content').innerHTML = `
        <h2 class="page-title">Domains</h2>
        <div class="card">
            ${domains.length > 0 ? `
            <table>
                <thead><tr><th>Domain</th><th>App</th><th>SSL</th><th>Status</th></tr></thead>
                <tbody>
                    ${Array.isArray(domains).map(d => `
                    <tr>
                        <td><strong>${escapeHtml(d.domain || d.name)}</strong></td>
                        <td>${escapeHtml(d.app_id || d.app || 'N/A')}</td>
                        <td><span class="badge ${d.ssl_enabled ? 'badge-success' : 'badge-warning'}">${d.ssl_enabled ? 'Enabled' : 'Disabled'}</span></td>
                        <td><span class="badge badge-${statusClass(d.status)}">${escapeHtml(d.status || 'unknown')}</span></td>
                    </tr>`).join('')}
                </tbody>
            </table>` : '<p class="muted">No domains configured.</p>'}
        </div>
    `;
}

// ── Databases ───────────────────────────────────────────────────────────────

async function loadDatabases() {
    const data = await apiFetch('/v1/databases').catch(() => ({ items: [] }));
    const databases = data.items || data.data || data || [];

    document.getElementById('main-content').innerHTML = `
        <h2 class="page-title">Databases</h2>
        <div class="card">
            ${databases.length > 0 ? `
            <table>
                <thead><tr><th>Name</th><th>Type</th><th>Size</th><th>Status</th></tr></thead>
                <tbody>
                    ${Array.isArray(databases).map(db => `
                    <tr>
                        <td><strong>${escapeHtml(db.name || db.id)}</strong></td>
                        <td>${escapeHtml(db.engine || db.type || 'N/A')}</td>
                        <td>${escapeHtml(db.size || 'N/A')}</td>
                        <td><span class="badge badge-${statusClass(db.status)}">${escapeHtml(db.status || 'unknown')}</span></td>
                    </tr>`).join('')}
                </tbody>
            </table>` : '<p class="muted">No databases created.</p>'}
        </div>
    `;
}

// ── Secrets ─────────────────────────────────────────────────────────────────

async function loadSecrets() {
    const data = await apiFetch('/v1/secrets').catch(() => ({ items: [] }));
    const secrets = data.items || data.data || data || [];

    document.getElementById('main-content').innerHTML = `
        <h2 class="page-title">Secrets</h2>
        <div class="card">
            ${secrets.length > 0 ? `
            <table>
                <thead><tr><th>Name</th><th>Created</th></tr></thead>
                <tbody>
                    ${Array.isArray(secrets).map(s => `
                    <tr>
                        <td><strong>${escapeHtml(s.name || s.id)}</strong></td>
                        <td>${s.created_at ? new Date(s.created_at).toLocaleDateString() : 'N/A'}</td>
                    </tr>`).join('')}
                </tbody>
            </table>` : '<p class="muted">No secrets stored.</p>'}
        </div>
    `;
}

// ── Branding ────────────────────────────────────────────────────────────────

async function loadBranding() {
    try {
        const branding = await fetch(`${API_BASE}/v1/config/branding`).then(r => r.json());
        const brand = branding.brand || branding;
        if (brand.name) {
            document.title = brand.name + ' Dashboard';
            document.getElementById('brand-name').textContent = brand.name;
            document.getElementById('nav-brand-name').textContent = brand.name;
        }
        if (brand.logo) {
            document.getElementById('brand-logo').src = brand.logo;
            document.getElementById('nav-logo').src = brand.logo;
        }
        if (brand.favicon) document.getElementById('favicon').href = brand.favicon;
        if (brand.primary_color) {
            document.documentElement.style.setProperty('--brand-primary', brand.primary_color);
        }
        if (brand.theme === 'light') {
            document.documentElement.setAttribute('data-theme', 'light');
        }
        if (brand.hide_powered_by) {
            const footer = document.querySelector('.brand-footer');
            if (footer) footer.remove();
        }
        if (brand.custom_footer) {
            const footer = document.querySelector('.brand-footer');
            if (footer) footer.textContent = brand.custom_footer;
        }
    } catch {
        console.warn('Failed to load branding, using defaults');
    }
}

// ── Utilities ───────────────────────────────────────────────────────────────

function statusClass(status) {
    if (!status) return 'warning';
    const s = status.toLowerCase();
    if (['running', 'online', 'ready', 'active', 'deployed', 'healthy', 'enabled'].includes(s)) return 'success';
    if (['error', 'failed', 'offline', 'draining', 'disabled'].includes(s)) return 'danger';
    return 'warning';
}

function escapeHtml(text) {
    if (!text) return '';
    const div = document.createElement('div');
    div.textContent = String(text);
    return div.innerHTML;
}

function showToast(message, type = 'success') {
    const container = document.getElementById('toast-container');
    const toast = document.createElement('div');
    toast.className = `toast toast-${type}`;
    toast.textContent = message;
    container.appendChild(toast);
    setTimeout(() => toast.remove(), 4000);
}

// ── Event Listeners ─────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    // Login form
    document.getElementById('login-form').addEventListener('submit', (e) => {
        e.preventDefault();
        const email = document.getElementById('email').value;
        const password = document.getElementById('password').value;
        login(email, password);
    });

    // Logout button
    document.getElementById('logout-btn').addEventListener('click', logout);

    // Sidebar navigation
    document.querySelectorAll('#sidebar li[data-section]').forEach(li => {
        li.addEventListener('click', () => loadSection(li.dataset.section));
    });

    // Load branding
    loadBranding();

    // Check for existing session
    const token = localStorage.getItem('shellwego_token');
    if (token) {
        showDashboard();
    } else {
        showLogin();
    }
});
