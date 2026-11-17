<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useToast } from 'primevue/usetoast';
import { Button, Column, DataTable, Dialog, InputText } from 'primevue';
import { Md5 } from 'ts-md5';
import ApiClient, { type AdminUserItem } from '../modules/api';

const { t } = useI18n();
const toast = useToast();

/**
 * 运营管理页：查看与管控所有用户。
 *
 * 入口为 `http://<host>:<port>/xlxxadmin`（后端会注入脚本跳转到 `#/xlxxadmin`）。
 * 该页面独立于主控制台布局，因此自行构造 ApiClient：
 * 优先使用后端下发的 api_host，否则回退到当前站点根路径。
 */
function resolveApiHost(): string {
    const meta = (window as any).apiMeta;
    if (meta?.api_host) {
        return String(meta.api_host).replace(/\/+$/, '');
    }
    return `${location.origin}`;
}

const api = new ApiClient(resolveApiHost(), () => {
    toast.add({ severity: 'error', summary: t('error'), detail: t('web.admin.login_required'), life: 3000 });
});

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
        const running = (resp?.running_inst_ids ?? []).map((id: any) => String(id));
        const disabled = (resp?.disabled_inst_ids ?? []).map((id: any) => String(id));
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
                    {{ data.device_count }}
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
                        <Button :label="t('web.admin.set_expire')" size="small" severity="secondary" outlined
                            @click="openExpireDialog(data)" />
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
</style>
