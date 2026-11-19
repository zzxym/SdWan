import axios, { AxiosError, AxiosInstance, AxiosResponse, InternalAxiosRequestConfig } from 'axios';
import { type Api, NetworkTypes, Utils } from 'sdwan-frontend-lib';
import { Md5 } from 'ts-md5';

export interface ValidateConfigResponse {
    toml_config: string;
}

export interface OidcConfigResponse {
    enabled: boolean;
}

// 定义接口返回的数据结构
export interface LoginResponse {
    success: boolean;
    message: string;
}

export interface RegisterResponse {
    success: boolean;
    message: string;
}

// 定义请求体数据结构
export interface Credential {
    username: string;
    password: string;
}

export interface RegisterData {
    credentials: Credential;
    captcha: string;
    captcha_id?: string;
}

// 验证码信息，captcha_id 需在注册时一并提交
export interface CaptchaInfo {
    captcha_id: string;
    image: string;
    expires_in: number;
}

export interface Summary {
    device_count: number;
}

/** 运营管理接口返回的用户条目。 */
export interface AdminUserItem {
    id: number;
    username: string;
    is_admin: boolean;
    enabled: boolean;
    /** 使用期限（Unix 秒），为空表示永久有效。 */
    expire_at: number | null;
    expired: boolean;
    device_count: number;
    /** 授权客户端数量上限，0 表示不限制。 */
    client_limit: number;
}

export interface ListAdminUsersResponse {
    users: AdminUserItem[];
}

export interface ListNetworkInstanceIdResponse {
    running_inst_ids: Array<Utils.UUID>,
    disabled_inst_ids: Array<Utils.UUID>,
}

export interface GenerateConfigRequest {
    config: NetworkTypes.NetworkConfig;
}

export interface GenerateConfigResponse {
    toml_config?: string;
    error?: string;
}

export interface ParseConfigRequest {
    toml_config: string;
}

export interface ParseConfigResponse {
    config?: NetworkTypes.NetworkConfig;
    error?: string;
}

export class ApiClient {
    private client: AxiosInstance;
    private authFailedCb: Function | undefined;

    constructor(baseUrl: string, authFailedCb: Function | undefined = undefined) {
        this.client = axios.create({
            baseURL: baseUrl.replace(/\/+$/, '') + '/api/v1',
            withCredentials: true, // 如果需要支持跨域携带cookie
            headers: {
                'Content-Type': 'application/json',
            },
        });
        this.authFailedCb = authFailedCb;

        // 添加请求拦截器
        this.client.interceptors.request.use((config: InternalAxiosRequestConfig) => {
            return config;
        }, (error: any) => {
            return Promise.reject(error);
        });

        // 添加响应拦截器
        this.client.interceptors.response.use((response: AxiosResponse) => {
            console.debug('Axios Response:', response);
            return response.data; // 假设服务器返回的数据都在data属性中
        }, (error: any) => {
            if (error.response) {
                let response: AxiosResponse = error.response;
                if (response.status == 401 && this.authFailedCb) {
                    console.error('Unauthorized:', response.data);
                    this.authFailedCb();
                } else {
                    // 请求已发出，但是服务器响应的状态码不在2xx范围
                    console.error('Response Error:', error.response.data);
                }
            } else if (error.request) {
                // 请求已发出，但是没有收到响应
                console.error('Request Error:', error.request);
            } else {
                // 发生了一些问题导致请求未发出
                console.error('Error:', error.message);
            }
            return Promise.reject(error);
        });
    }

    // 注册
    public async register(data: RegisterData): Promise<RegisterResponse> {
        try {
            data.credentials.password = Md5.hashStr(data.credentials.password);
            const response = await this.client.post<RegisterResponse>('/auth/register', data);
            console.log("register response:", response);
            return { success: true, message: 'Register success', };
        } catch (error) {
            if (error instanceof AxiosError) {
                return { success: false, message: 'Failed to register, error: ' + JSON.stringify(error.response?.data), };
            }
            return { success: false, message: 'Unknown error, error: ' + error, };
        }
    }

    // 登录
    public async login(data: Credential): Promise<LoginResponse> {
        try {
            data.password = Md5.hashStr(data.password);
            const response = await this.client.post<any>('/auth/login', data);
            console.log("login response:", response);
            return { success: true, message: 'Login success', };
        } catch (error) {
            if (error instanceof AxiosError) {
                const status = error.response?.status;
                const msg = String(error.response?.data ?? '');
                if (status === 401) {
                    return { success: false, message: 'auth.invalid_credentials', };
                } else if (status === 403) {
                    if (msg.includes('disabled')) {
                        return { success: false, message: 'auth.account_disabled', };
                    }
                    if (msg.includes('expired')) {
                        return { success: false, message: 'auth.account_expired', };
                    }
                }
                return { success: false, message: 'auth.login_failed', };
            }
            return { success: false, message: 'auth.login_failed', };
        }
    }

    public async logout() {
        await this.client.get('/auth/logout');
        if (this.authFailedCb) {
            this.authFailedCb();
        }
    }

    public async change_password(new_password: string) {
        await this.client.put('/auth/password', { new_password: Md5.hashStr(new_password) });
    }

    public async check_login_status() {
        try {
            await this.client.get('/auth/check_login_status');
            return true;
        } catch (error) {
            return false;
        }
    }

    public async list_session() {
        const response = await this.client.get('/sessions');
        return response;
    }

    public async list_machines(): Promise<Array<any>> {
        const response = await this.client.get<any, Record<string, Array<any>>>('/machines');
        return response.machines;
    }

    public async get_summary(): Promise<Summary> {
        const response = await this.client.get<any, Summary>('/summary');
        return response;
    }

    // ---------------- 运营管理接口 ----------------

    /** 列出全部用户及其管理状态（需要管理员权限）。 */
    public async admin_list_users(): Promise<AdminUserItem[]> {
        const response = await this.client.get<any, ListAdminUsersResponse>('/admin/users');
        return response.users;
    }

    /** 提权 / 降权：把用户加入或移出 admins 组。 */
    public async admin_set_role(username: string, is_admin: boolean) {
        await this.client.post(`/admin/users/${encodeURIComponent(username)}/role`, { is_admin });
    }

    /** 启用 / 禁用账号。 */
    public async admin_set_enabled(username: string, enabled: boolean) {
        await this.client.post(`/admin/users/${encodeURIComponent(username)}/enabled`, { enabled });
    }

    /** 设置使用期限（Unix 秒），传 null 表示永久有效。 */
    public async admin_set_expire(username: string, expire_at: number | null) {
        await this.client.post(`/admin/users/${encodeURIComponent(username)}/expire`, { expire_at });
    }

    /** 设置授权客户端数量上限，0 表示不限制。 */
    public async admin_set_client_limit(username: string, client_limit: number) {
        await this.client.post(`/admin/users/${encodeURIComponent(username)}/client-limit`, { client_limit });
    }

    /** 重置指定用户的密码。 */
    public async admin_reset_password(username: string, new_password: string) {
        await this.client.post(`/admin/users/${encodeURIComponent(username)}/password`, { new_password });
    }

    /** 列出指定用户的机器（客户端）。 */
    public async admin_list_user_machines(username: string): Promise<Array<any>> {
        const response = await this.client.get<any, { machines: Array<any> }>(
            `/admin/users/${encodeURIComponent(username)}/machines`);
        return response.machines;
    }

    /** 踢出指定用户的一台在线客户端。 */
    public async admin_kick_machine(username: string, machine_id: string): Promise<{ success: boolean }> {
        const response = await this.client.post<any, { success: boolean }>(
            `/admin/users/${encodeURIComponent(username)}/machines/${machine_id}/kick`, {});
        return response;
    }

    /** 列出指定用户在某台机器上的网络实例 id。 */
    public async admin_list_user_networks(username: string, machine_id: string): Promise<ListNetworkInstanceIdResponse> {
        const response = await this.client.get<any, ListNetworkInstanceIdResponse>(
            `/admin/users/${encodeURIComponent(username)}/machines/${machine_id}/networks`);
        return response;
    }

    /** 删除指定用户的网络实例。 */
    public async admin_delete_user_network(username: string, machine_id: string, inst_id: string) {
        await this.client.delete(
            `/admin/users/${encodeURIComponent(username)}/machines/${machine_id}/networks/${inst_id}`);
    }

    /** 启用 / 停用指定用户的网络实例。 */
    public async admin_update_user_network_state(
        username: string, machine_id: string, inst_id: string, disabled: boolean) {
        await this.client.put(
            `/admin/users/${encodeURIComponent(username)}/machines/${machine_id}/networks/${inst_id}`,
            { disabled });
    }

    /** 读取指定用户的网络实例配置。 */
    public async admin_get_user_network_config(
        username: string, machine_id: string, inst_id: string): Promise<NetworkTypes.NetworkConfig> {
        const response = await this.client.get<any, NetworkTypes.NetworkConfig>(
            `/admin/users/${encodeURIComponent(username)}/machines/${machine_id}/networks/config/${inst_id}`);
        return NetworkTypes.normalizeNetworkConfig(response);
    }

    /** 保存指定用户的网络实例配置。 */
    public async admin_save_user_network_config(
        username: string, machine_id: string, config: NetworkTypes.NetworkConfig) {
        await this.client.put(
            `/admin/users/${encodeURIComponent(username)}/machines/${machine_id}/networks/config/${config.instance_id}`,
            { config: NetworkTypes.toBackendNetworkConfig(config) });
    }

    /** 为指定用户新增并运行一个网络实例。 */
    public async admin_run_user_network(
        username: string, machine_id: string, config: NetworkTypes.NetworkConfig, save = true) {
        await this.client.post(
            `/admin/users/${encodeURIComponent(username)}/machines/${machine_id}/networks`,
            { config: NetworkTypes.toBackendNetworkConfig(config), save });
    }

    public captcha_url() {
        return this.client.defaults.baseURL + '/auth/captcha';
    }

    // 获取验证码（含 captcha_id），注册时一并提交，校验不依赖 session cookie
    public async get_captcha(): Promise<CaptchaInfo> {
        const response = await this.client.get<any, CaptchaInfo>('/auth/captcha_info');
        return response;
    }

    public async getOidcConfig(): Promise<OidcConfigResponse> {
        try {
            const response = await this.client.get<any, OidcConfigResponse>('/auth/oidc/config');
            return response;
        } catch (error) {
            return { enabled: false };
        }
    }

    public oidcLoginUrl() {
        return this.client.defaults.baseURL + '/auth/oidc/login';
    }

    public get_remote_client(machine_id: string): Api.RemoteClient {
        return new WebRemoteClient(machine_id, this.client);
    }
}

class WebRemoteClient implements Api.RemoteClient {
    private machine_id: string;
    private client: AxiosInstance;

    constructor(machine_id: string, client: AxiosInstance) {
        this.machine_id = machine_id;
        this.client = client;
    }
    async validate_config(config: NetworkTypes.NetworkConfig): Promise<Api.ValidateConfigResponse> {
        const response = await this.client.post<NetworkTypes.NetworkConfig, ValidateConfigResponse>(`/machines/${this.machine_id}/validate-config`, {
            config: NetworkTypes.toBackendNetworkConfig(config),
        });
        return response;
    }
    async run_network(config: NetworkTypes.NetworkConfig, save: boolean): Promise<undefined> {
        await this.client.post<string>(`/machines/${this.machine_id}/networks`, {
            config: NetworkTypes.toBackendNetworkConfig(config),
            save: save
        });
    }
    async get_network_info(inst_id: string): Promise<NetworkTypes.NetworkInstanceRunningInfo | undefined> {
        const response = await this.client.get<any, Api.CollectNetworkInfoResponse>('/machines/' + this.machine_id + '/networks/info/' + inst_id);
        return response.info.map[inst_id];
    }
    async list_network_instance_ids(): Promise<Api.ListNetworkInstanceIdResponse> {
        const response = await this.client.get<any, ListNetworkInstanceIdResponse>('/machines/' + this.machine_id + '/networks');
        return response;
    }
    async delete_network(inst_id: string): Promise<undefined> {
        await this.client.delete<string>(`/machines/${this.machine_id}/networks/${inst_id}`);
    }
    async update_network_instance_state(inst_id: string, disabled: boolean): Promise<undefined> {
        await this.client.put<string>('/machines/' + this.machine_id + '/networks/' + inst_id, {
            disabled: disabled,
        });
    }
    async save_config(config: NetworkTypes.NetworkConfig): Promise<undefined> {
        await this.client.put(`/machines/${this.machine_id}/networks/config/${config.instance_id}`, {
            config: NetworkTypes.toBackendNetworkConfig(config)
        });
    }
    async get_network_config(inst_id: string): Promise<NetworkTypes.NetworkConfig> {
        const response = await this.client.get<any, NetworkTypes.NetworkConfig>('/machines/' + this.machine_id + '/networks/config/' + inst_id);
        return NetworkTypes.normalizeNetworkConfig(response);
    }
    async generate_config(config: NetworkTypes.NetworkConfig): Promise<Api.GenerateConfigResponse> {
        try {
            const response = await this.client.post<any, GenerateConfigResponse>('/generate-config', {
                config: NetworkTypes.toBackendNetworkConfig(config)
            });
            return response;
        } catch (error) {
            if (error instanceof AxiosError) {
                return { error: error.response?.data };
            }
            return { error: 'Unknown error: ' + error };
        }
    }
    async parse_config(toml_config: string): Promise<Api.ParseConfigResponse> {
        try {
            const response = await this.client.post<any, ParseConfigResponse>('/parse-config', { toml_config });
            if (response.config) {
                response.config = NetworkTypes.normalizeNetworkConfig(response.config);
            }
            return response;
        } catch (error) {
            if (error instanceof AxiosError) {
                return { error: error.response?.data };
            }
            return { error: 'Unknown error: ' + error };
        }
    }
    async get_network_metas(instance_ids: string[]): Promise<Api.GetNetworkMetasResponse> {
        const response = await this.client.post<any, Api.GetNetworkMetasResponse>(`/machines/${this.machine_id}/networks/metas`, {
            instance_ids: instance_ids
        });
        return response;
    }
}

export default ApiClient;
