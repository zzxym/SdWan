<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { useToast } from 'primevue/usetoast';
import { Button, Column, DataTable, Dialog, InputText, TieredMenu } from 'primevue';
import { Md5 } from 'ts-md5';
import { I18nUtils, Utils } from 'sdwan-frontend-lib';
import Icon from '../assets/sdwan.png';
import ApiClient, { clearNetworkCredentials, type AdminUserItem } from '../modules/api';

const { t } = useI18n();
const toast = useToast();
const router = useRouter();

/**
 * 运营管理页：查看与管控所有用户。
 *
 * 入口为 `http://<host>:<port>/xlxxadmin`（后端会注入脚本跳转到 `/#/xlxxadmin`）。
 * 该页面独立于主控制台布局，因此自行构造 ApiClient：
 * 1) 优先使用用户在登录页选择并保存的 apiHost（localStorage），避免 apiMeta 配错时全部接口打到静态站；
 * 2) 其次使用后端下发的 api_host；
 * 3) 最后回退到当前站点根路径。
 */
function resolveApiHost(): string {
    try {
        const saved = JSON.parse(localStorage.getItem('apiHosts') || '[]');
        if (Array.isArray(saved) && saved.length > 0 && saved[0]?.value) {
            return String(saved[0].value).replace(/\/+$/, '');
        }
    } catch (_) {
        // ignore malformed localStorage
    }
    const meta = (window as any).apiMeta;
    if (meta?.api_host) {
        return String(meta.api_host).replace(/\/+$/, '');
    }
    return `${location.origin}`;
}

const api = new ApiClient(resolveApiHost(), () => {
    toast.add({ severity: 'error', summary: t('error'), detail: t('web.admin.login_required'), life: 3000 });
});

// 页头用户菜单：返回控制台 / 退出登录
const userMenu = ref();
const userMenuItems = computed(() => [
    {
        label: t('web.main.back_to_console'),
        icon: 'pi pi-arrow-left',
        command: () => {
            const apiHost = localStorage.getItem('apiHost');
            if (apiHost) {
                router.push({ name: 'dashboard', params: { apiHost } });
            } else {
                router.push({ name: 'login' });
            }
        },
    },
    {
        label: t('web.main.logout'),
        icon: 'pi pi-sign-out',
        command: async () => {
            try {
                await api.logout();
            } catch (e) {
                console.error('logout failed', e);
            }
            clearNetworkCredentials();
            router.push({ name: 'login' });
        },
    },
]);

const loading = ref(false);
const users = ref<AdminUserItem[]>([]);

// ---------------------------------------------------------------- 期限设置
const expireDialogVisible = ref(false);
const expireTarget = ref<AdminUserItem | null>(null);
/** 距今天数，0 表示永久有效。 */
const expireDays = ref<string>('0');

function openExpireDialog(user: AdminUserItem) {
    expireTarget.value = user;
    expireDays.value = '0';
    expireDialogVisible.value = true;
}

async function confirmExpire() {
    if (!expireTarget.value) {
        return;
    }
    const username = expireTarget.value.username;
    // 传 null 表示永久有效；否则按"当前时间 + N 天"计算到期时间戳（秒）。
    const days = Number(expireDays.value) || 0;
    const expire_at = days > 0
        ? Math.floor(Date.now() / 1000) + days * 86400
        : null;
    try {
        await api.admin_set_expire(username, expire_at);
        toast.add({ severity: 'success', summary: t('web.admin.save_success'), life: 2000 });
        expireDialogVisible.value = false;
        await loadUsers();
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    }
}

// ---------------------------------------------------------------- 授权客户端数设置
const clientLimitDialogVisible = ref(false);
const clientLimitTarget = ref<AdminUserItem | null>(null);
/** 授权客户端数，0 表示不限制。 */
const clientLimitValue = ref<string>('0');

function openClientLimitDialog(user: AdminUserItem) {
    clientLimitTarget.value = user;
    clientLimitValue.value = String(user.client_limit ?? 0);
    clientLimitDialogVisible.value = true;
}

async function confirmClientLimit() {
    if (!clientLimitTarget.value) {
        return;
    }
    const limit = Number(clientLimitValue.value);
    if (!Number.isInteger(limit) || limit < 0) {
        toast.add({ severity: 'error', summary: t('error'), detail: t('web.admin.client_limit_hint'), life: 3000 });
        return;
    }
    const username = clientLimitTarget.value.username;
    try {
        await api.admin_set_client_limit(username, limit);
        toast.add({ severity: 'success', summary: t('web.admin.save_success'), life: 2000 });
        clientLimitDialogVisible.value = false;
        await loadUsers();
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    }
}

// ---------------------------------------------------------------- 密码重置
const passwordDialogVisible = ref(false);
const passwordTarget = ref<AdminUserItem | null>(null);
const newPassword = ref('');

function openPasswordDialog(user: AdminUserItem) {
    passwordTarget.value = user;
    newPassword.value = '';
    passwordDialogVisible.value = true;
}

async function confirmResetPassword() {
    if (!passwordTarget.value || !newPassword.value) {
        return;
    }
    const username = passwordTarget.value.username;
    try {
        // 服务端按既有约定（MD5 + argon2）哈希后存储，这里提交 MD5 后的值。
        await api.admin_reset_password(username, Md5.hashStr(newPassword.value));
        toast.add({ severity: 'success', summary: t('web.admin.save_success'), life: 2000 });
        passwordDialogVisible.value = false;
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    }
}

// ---------------------------------------------------------------- 网络管控
const networkDialogVisible = ref(false);
const networkTarget = ref<AdminUserItem | null>(null);
const networkMachines = ref<Array<any>>([]);
const networkMachineId = ref<string>('');
const networkInstances = ref<Array<{ id: string, running: boolean }>>([]);
const networkLoading = ref(false);

function machineIdOf(machine: any): string {
    return machine?.machine_id
        ?? machine?.info?.machine_id
        ?? (machine?.client_url ? String(machine.client_url).split('/').pop() : '');
}

async function openNetworkDialog(user: AdminUserItem) {
    networkTarget.value = user;
    networkDialogVisible.value = true;
    networkMachines.value = [];
    networkMachineId.value = '';
    networkInstances.value = [];
    await loadUserMachines();
}

async function loadUserMachines() {
    if (!networkTarget.value) {
        return;
    }
    networkLoading.value = true;
    try {
        const machines = await api.admin_list_user_machines(networkTarget.value.username);
        networkMachines.value = machines;
        if (machines.length > 0) {
            networkMachineId.value = machineIdOf(machines[0]);
            await loadUserNetworks();
        } else {
            networkMachineId.value = '';
            networkInstances.value = [];
        }
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    } finally {
        networkLoading.value = false;
    }
}

async function loadUserNetworks() {
    if (!networkTarget.value || !networkMachineId.value) {
        networkInstances.value = [];
        return;
    }
    networkLoading.value = true;
    try {
        const resp = await api.admin_list_user_networks(networkTarget.value.username, networkMachineId.value);
        // 后端返回 protobuf Uuid {part1..4} 对象，需用 Utils.UuidToStr 转成标准 UUID 字符串，
        // 否则 String(id) 会得到 [object Object]，且后续启停/删除接口无法识别。
        const running = (resp?.running_inst_ids ?? []).map((id: any) => Utils.UuidToStr(id));
        const disabled = (resp?.disabled_inst_ids ?? []).map((id: any) => Utils.UuidToStr(id));
        networkInstances.value = [
            ...running.map((id) => ({ id, running: true })),
            ...disabled.map((id) => ({ id, running: false })),
        ];
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    } finally {
        networkLoading.value = false;
    }
}

async function toggleNetworkInstance(inst: { id: string, running: boolean }) {
    if (!networkTarget.value || !networkMachineId.value) {
        return;
    }
    try {
        await api.admin_update_user_network_state(
            networkTarget.value.username, networkMachineId.value, inst.id, inst.running);
        await loadUserNetworks();
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    }
}

async function deleteNetworkInstance(inst: { id: string }) {
    if (!networkTarget.value || !networkMachineId.value) {
        return;
    }
    try {
        await api.admin_delete_user_network(
            networkTarget.value.username, networkMachineId.value, inst.id);
        toast.add({ severity: 'success', summary: t('web.admin.delete_success'), life: 2000 });
        await loadUserNetworks();
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    }
}

/** 踢出当前选中的在线客户端（断开其与配置服务器的会话）。 */
async function kickMachine() {
    if (!networkTarget.value || !networkMachineId.value) {
        return;
    }
    try {
        await api.admin_kick_machine(networkTarget.value.username, networkMachineId.value);
        toast.add({ severity: 'success', summary: t('web.admin.kick_success'), life: 2000 });
        await loadUserMachines();
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    }
}

// ---------------------------------------------------------------- 用户操作
async function loadUsers() {
    loading.value = true;
    try {
        users.value = await api.admin_list_users();
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 5000,
        });
    } finally {
        loading.value = false;
    }
}

async function toggleRole(user: AdminUserItem) {
    try {
        await api.admin_set_role(user.username, !user.is_admin);
        toast.add({ severity: 'success', summary: t('web.admin.save_success'), life: 2000 });
        await loadUsers();
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    }
}

async function toggleEnabled(user: AdminUserItem) {
    try {
        await api.admin_set_enabled(user.username, !user.enabled);
        toast.add({ severity: 'success', summary: t('web.admin.save_success'), life: 2000 });
        await loadUsers();
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    }
}

// ---------------------------------------------------------------- 删除账户
const deleteDialogVisible = ref(false);
const deleteTarget = ref<AdminUserItem | null>(null);
const adminPassword = ref('');
const deleting = ref(false);

function openDeleteDialog(user: AdminUserItem) {
    deleteTarget.value = user;
    adminPassword.value = '';
    deleteDialogVisible.value = true;
}

async function confirmDeleteUser() {
    if (!deleteTarget.value || !adminPassword.value) {
        return;
    }
    const username = deleteTarget.value.username;
    deleting.value = true;
    try {
        // 服务端要求校验当前管理员密码，按既有约定提交 MD5 后的值。
        await api.admin_delete_user(username, Md5.hashStr(adminPassword.value));
        toast.add({ severity: 'success', summary: t('web.admin.delete_success'), life: 2000 });
        deleteDialogVisible.value = false;
        await loadUsers();
    } catch (e: any) {
        toast.add({
            severity: 'error',
            summary: t('error'),
            detail: e?.response?.data?.message ?? String(e),
            life: 3000,
        });
    } finally {
        deleting.value = false;
    }
}

function formatExpire(expire_at: number | null): string {
    if (!expire_at) {
        return t('web.admin.never_expire');
    }
    return new Date(expire_at * 1000).toLocaleString();
}

const machineOptions = computed(() => networkMachines.value.map((m: any) => ({
    value: machineIdOf(m),
    label: m?.info?.hostname ?? m?.client_url ?? m?.machine_id ?? '',
})));

onMounted(loadUsers);
</script>

<template>
    <!-- 页头：与控制台一致的 logo + 语言切换 + 用户菜单 -->
    <nav class="admin-top-nav">
        <div class="admin-top-nav-inner">
            <a href="https://sdwan.top" class="flex items-center">
                <img :src="Icon" class="h-9 me-3" alt="Sdwan Logo" />
                <span class="self-center text-xl font-semibold whitespace-nowrap">Sdwan</span>
            </a>
            <div class="flex items-center gap-3">
                <Button icon="pi pi-language" @click="I18nUtils.toggleLanguage" rounded severity="contrast" />
                <Button type="button" @click="userMenu.toggle($event)" aria-haspopup="true"
                    aria-controls="admin-user-menu" icon="pi pi-user" raised rounded />
                <TieredMenu ref="userMenu" id="admin-user-menu" :model="userMenuItems" popup />
            </div>
        </div>
    </nav>
    <div class="admin-page">
        <div class="flex items-center justify-between mb-4">
            <h2 class="text-xl font-semibold">{{ t('web.admin.title') }}</h2>
            <Button icon="pi pi-refresh" severity="secondary" text rounded :loading="loading"
                @click="loadUsers" />
        </div>

        <DataTable :value="users" :loading="loading" dataKey="id" stripedRows responsiveLayout="scroll"
            :emptyMessage="t('web.admin.no_users')">
            <Column :header="t('web.admin.username')" field="username">
                <template #body="{ data }">
                    <div class="flex items-center gap-2">
                        <span class="font-medium">{{ data.username }}</span>
                        <span v-if="data.is_admin" class="admin-badge">{{ t('web.admin.admin') }}</span>
                    </div>
                </template>
            </Column>

            <Column :header="t('web.admin.status')" style="width: 9rem">
                <template #body="{ data }">
                    <span v-if="!data.enabled" class="status-disabled">{{ t('web.admin.disabled') }}</span>
                    <span v-else-if="data.expired" class="status-expired">{{ t('web.admin.expired') }}</span>
                    <span v-else class="status-active">{{ t('web.admin.active') }}</span>
                </template>
            </Column>

            <Column :header="t('web.admin.expire_at')" style="width: 14rem">
                <template #body="{ data }">
                    {{ formatExpire(data.expire_at) }}
                </template>
            </Column>

            <Column :header="t('web.admin.device_count')" style="width: 8rem">
                <template #body="{ data }">
                    {{ data.device_count }}/{{ data.client_limit > 0 ? data.client_limit : '∞' }}
                </template>
            </Column>

            <Column :header="t('web.admin.actions')">
                <template #body="{ data }">
                    <div class="flex flex-wrap gap-2">
                        <Button :label="data.is_admin ? t('web.admin.demote') : t('web.admin.promote')"
                            size="small" severity="secondary" outlined @click="toggleRole(data)" />
                        <Button :label="data.enabled ? t('web.admin.disable') : t('web.admin.enable')"
                            size="small" :severity="data.enabled ? 'danger' : 'success'" outlined
                            @click="toggleEnabled(data)" />
                        <Button :label="t('web.admin.delete_account')" size="small" severity="danger" outlined
                            @click="openDeleteDialog(data)" />
                        <Button :label="t('web.admin.set_expire')" size="small" severity="secondary" outlined
                            @click="openExpireDialog(data)" />
                        <Button :label="t('web.admin.set_client_limit')" size="small" severity="secondary" outlined
                            @click="openClientLimitDialog(data)" />
                        <Button :label="t('web.admin.reset_password')" size="small" severity="secondary" outlined
                            @click="openPasswordDialog(data)" />
                        <Button :label="t('web.admin.manage_networks')" size="small" outlined
                            @click="openNetworkDialog(data)" />
                    </div>
                </template>
            </Column>
        </DataTable>

        <!-- 使用期限 -->
        <Dialog v-model:visible="expireDialogVisible" :header="t('web.admin.set_expire')" modal>
            <div class="flex flex-col gap-3">
                <p class="text-sm text-surface-500">{{ t('web.admin.expire_hint') }}</p>
                <div class="flex items-center gap-3">
                    <label for="expire-days" class="text-sm">{{ t('web.admin.expire_days') }}</label>
                    <InputText id="expire-days" v-model="expireDays" type="number" min="0" class="w-32" />
                    <span class="text-sm text-surface-500">{{ t('web.admin.days') }}</span>
                </div>
            </div>
            <template #footer>
                <Button :label="t('web.common.cancel')" severity="secondary" text
                    @click="expireDialogVisible = false" />
                <Button :label="t('web.common.save')" @click="confirmExpire" />
            </template>
        </Dialog>

        <!-- 授权客户端数 -->
        <Dialog v-model:visible="clientLimitDialogVisible" :header="t('web.admin.set_client_limit')" modal>
            <div class="flex flex-col gap-3">
                <p class="text-sm text-surface-500">{{ t('web.admin.client_limit_hint') }}</p>
                <div class="flex items-center gap-3">
                    <label for="client-limit" class="text-sm">{{ t('web.admin.client_limit') }}</label>
                    <InputText id="client-limit" v-model="clientLimitValue" type="number" min="0" class="w-32" />
                </div>
            </div>
            <template #footer>
                <Button :label="t('web.common.cancel')" severity="secondary" text
                    @click="clientLimitDialogVisible = false" />
                <Button :label="t('web.common.save')" @click="confirmClientLimit" />
            </template>
        </Dialog>

        <!-- 重置密码 -->
        <Dialog v-model:visible="passwordDialogVisible" :header="t('web.admin.reset_password')" modal>
            <div class="flex flex-col gap-3">
                <label for="new-password" class="text-sm">{{ t('web.admin.new_password') }}</label>
                <InputText id="new-password" v-model="newPassword" type="password" class="w-full" />
            </div>
            <template #footer>
                <Button :label="t('web.common.cancel')" severity="secondary" text
                    @click="passwordDialogVisible = false" />
                <Button :label="t('web.common.save')" @click="confirmResetPassword" />
            </template>
        </Dialog>

        <!-- 删除账户 -->
        <Dialog v-model:visible="deleteDialogVisible" :header="t('web.admin.delete_account')" modal>
            <div class="flex flex-col gap-3">
                <p class="text-sm text-surface-500">{{ t('web.admin.delete_account_hint') }}</p>
                <label for="admin-password" class="text-sm">{{ t('web.admin.admin_password') }}</label>
                <InputText id="admin-password" v-model="adminPassword" type="password" class="w-full" />
            </div>
            <template #footer>
                <Button :label="t('web.common.cancel')" severity="secondary" text
                    @click="deleteDialogVisible = false" />
                <Button :label="t('web.admin.confirm_delete')" severity="danger" :loading="deleting"
                    @click="confirmDeleteUser" />
            </template>
        </Dialog>

        <!-- 网络实例管控 -->
        <Dialog v-model:visible="networkDialogVisible" :header="t('web.admin.manage_networks')" modal
            :style="{ width: '46rem' }">
            <div class="flex flex-col gap-4">
                <div v-if="machineOptions.length > 1" class="flex flex-col gap-2">
                    <label for="machine-select" class="text-sm">{{ t('web.admin.machine') }}</label>
                    <select id="machine-select" v-model="networkMachineId" class="machine-select"
                        @change="loadUserNetworks">
                        <option v-for="opt in machineOptions" :key="opt.value" :value="opt.value">
                            {{ opt.label }}
                        </option>
                    </select>
                </div>

                <div v-if="networkLoading" class="text-sm text-surface-500">{{ t('web.common.loading') }}</div>

                <div v-else-if="networkInstances.length === 0" class="text-sm text-surface-500">
                    {{ t('web.admin.no_networks') }}
                </div>

                <ul v-else class="flex flex-col gap-2">
                    <li v-for="inst in networkInstances" :key="inst.id"
                        class="network-item">
                        <span class="font-mono text-xs break-all">{{ inst.id }}</span>
                        <div class="flex items-center gap-2">
                            <Button :label="inst.running ? t('web.admin.stop') : t('web.admin.start')" size="small"
                                :severity="inst.running ? 'danger' : 'success'" outlined
                                @click="toggleNetworkInstance(inst)" />
                            <Button icon="pi pi-trash" size="small" severity="danger" outlined rounded
                                @click="deleteNetworkInstance(inst)" />
                        </div>
                    </li>
                </ul>
            </div>
            <template #footer>
                <Button :label="t('web.admin.kick_client')" severity="danger" outlined
                    :disabled="!networkMachineId" @click="kickMachine" />
                <Button :label="t('web.common.close')" severity="secondary"
                    @click="networkDialogVisible = false" />
            </template>
        </Dialog>
    </div>
</template>

<style scoped>
.admin-badge {
    padding: 0.1rem 0.45rem;
    font-size: 0.7rem;
    border-radius: 0.75rem;
    color: #166534;
    background: #dcfce7;
}

.status-active {
    color: #166534;
}

.status-disabled {
    color: #991b1b;
}

.status-expired {
    color: #b45309;
}

.network-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.5rem;
    border: 1px solid var(--p-surface-200);
    border-radius: 0.5rem;
}

.machine-select {
    padding: 0.5rem 0.75rem;
    font-size: 0.9rem;
    color: inherit;
    background: transparent;
    border: 1px solid var(--p-surface-300);
    border-radius: 0.5rem;
}

.admin-top-nav {
    position: sticky;
    top: 0;
    z-index: 50;
    width: 100%;
    background: var(--p-surface-0, #fff);
    border-bottom: 1px solid var(--p-surface-200, #e5e7eb);
}

.admin-top-nav-inner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.6rem 1.25rem;
}
</style>
