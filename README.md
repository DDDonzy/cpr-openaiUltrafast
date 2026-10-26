# CPR Ultrafast 服务档位

按指定 Key、上游账号和模型，在 CPR 的 `attempt` 阶段强制把顶层 `service_tier` 改为 `ultrafast`。客户端不用开启 Fast，客户端显式选择的 `default`、`priority`、`fast`、`flex` 等也会被覆盖。插件不改变账号路由、响应正文或上游回显，不能因此保证账号拥有加速权益。

## 当前版本与边界

- 版本：**0.2.3**
- 声明宿主范围：`>=3.32.0, <4.0.0`，RPC v6 / middleware v6 / management v2
- SDK 固定于官方 v3.32.0，未改 SDK 实现
- Linux ARM64 musl 安装包，目标为 Linux ARM64 环境
- 2026-10-26 针对 CPR 3.32.0 的协议 v6 兼容性重建 0.2.3；保持规则、入口绑定及私有状态格式不变，未测试真实上游加速权益
- 上述版本范围是加载约束，不承诺所有未来 3.x 宿主都已经测试

## 配置入口

插件实例的 `configuration` 为 `{}`，业务规则在独立菜单 **插件 → Ultrafast 服务档位** 管理，不再在实例设置里填写 JSON。

界面参考“账号对话测试”：三栏原生卡片分别用于 Key、账号和规则设置，下方使用原生规则表格。直接在三栏中创建或编辑规则，“新建规则”会重置草稿并聚焦名称；已有未保存修改时先确认，不再打开长表单弹窗。运行版本、排空修订和入口限制说明集中在“运行详情”。

规则保存到插件私有状态 `ultrafast-rules/document`，不会写入账号、分组或客户端设置。新实例没有规则时不改写任何请求。若旧实例仍携带 0.1.x 配置，只有私有状态尚不存在时才沿用旧规则，已经持久化的空规则优先，不会恢复旧配置。

页面显示三个不同事实：

1. 实际包版本 `pluginVersion`，如 `0.2.1`
2. 规则状态版本 `version`，用于精确并发控制
3. 宿主实例是否 enabled / running，以及原生入口绑定、兼容提示和排空中的旧 revision

## 规则与原生入口绑定

生效范围是 **原生绑定 ∩ 插件规则**，不是仅看页面是否保存成功。

- 多条规则取并集
- 同一规则的 Key、账号、模型条件取交集
- 账号留空：不额外限制账号，仍使用 Key 有权访问的分组
- Key 留空：不额外限制 Key，但此时必须指定账号，且原生入口需要允许全部 Key
- 模型只允许 `gpt-6-astra`、`gpt-6.1-sol`
- 只处理 OpenAI provider、OpenAI protocol 的 `generate` 操作

原生绑定示例：

```json
{
  "contribution": "donzy.ultrafast.middleware",
  "stage": "attempt",
  "order": 100,
  "failurePolicy": "reject",
  "clientKeyIds": ["key_replace_me"],
  "accountGroupIds": [],
  "providerIds": ["openai"],
  "models": ["gpt-6-astra", "gpt-6.1-sol"],
  "identityBindings": []
}
```

管理页明确显示当前入口限制，不会自动修改或扩大生产绑定。入口外的 Key 在选择器中不可选，服务端也会拒绝入口未覆盖的 Key / 模型组合、已不存在的 Key 或非 OpenAI 账号。若原生绑定还有分组或身份条件，页面会额外提示，这些条件仍由宿主执行。

## 保存、错误和缓存

- 保存必须提供 `expectedVersion`，`null` 只表示创建尚不存在的记录
- 非法字段类型、未知字段、重复 ID、非法模型、空身份范围或超限文档会在持久化前拒绝
- 最多 64 条规则，每个身份维度最多 128 个 ID，总文档最多 60 KiB
- 前端等待服务端确认后才更新已保存列表，保存失败保留编辑草稿和错误
- 并发冲突或响应丢失不会自动重试，也不会用旧页面内容覆盖其他人的配置，须显式刷新核对
- 当前插件进程的保存和缓存刷新共用异步锁，保存成功立即替换缓存
- 其他仍在排空中的进程最多保留短期缓存，已开始的请求不能撤回，不能把当前 `running` 理解成旧进程都已经退出
- 状态读取异常时沿用实例的 `reject` 失败策略，不使用陈旧规则冒充保存成功

## 改写与停用

仅替换顶层档位值或插入缺失字段，保留其他原始 JSON 字节、数值精度和嵌套字段。已是 `ultrafast` 时不重编码，顶层重复档位键或命中范围的非法 JSON 在发往上游前拒绝。

空规则只代表 **不再由插件强制 Ultrafast**，并不阻止客户端自己发送 `service_tier: ultrafast`。如需全局禁止客户端使用该档位，需要另行定义和授权请求策略。

在宿主插件管理中停用实例会撤回后续请求的插件策略，也会关闭它的管理页面。无需为此重启 CPR。不要未经核验强杀排空进程或宣称其对现有会话没有影响。

## 构建与本地验证

需要 Rust 1.99+、Node 24+、Python 3 及 Rust `aarch64-unknown-linux-musl` 目标。使用前端锁文件安装依赖后运行：

```powershell
npm --prefix frontend ci --ignore-scripts
python Scripts/build.py
```

构建入口执行 Vue 严格模板检查、Vite、前端状态测试、Rust fmt / Clippy / 测试、真实插件进程的中间件与管理接口测试，再打包 ARM64 制品。安装包仅包含 `plugin.json`、`bin/plugin` 与声明的 web 资源，不包含测试夹具或凭据。

隔离页面验收：

```powershell
python Scripts/preview.py
```

浏览器打开 `http://127.0.0.1:18949/embedded`，浅色主题加 `?theme=light`。预览通过真实本地插件进程执行管理请求，但账号、Key、绑定与状态均为虚构内存数据，不连接生产 CPR 或模型，也不能代替目标宿主验收。

产物为 `dist/donzy.ultrafast-0.2.3-aarch64-unknown-linux-musl.tar.gz` 及对应 SHA256 文件。生产升级前应备份实例与私有状态；插件不会自动恢复或修改账号的加速规则。
