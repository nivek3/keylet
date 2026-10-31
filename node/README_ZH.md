# Keylet Node.js SDK

[English](README.md) | **中文** · [Keylet 项目说明](../README_ZH.md) · [构建与签名](../docs/signing.md)

`keylet` 让 Node.js 脚本通过已签名的 macOS App `Keylet.app` 获取派生公私钥。App 通过 Touch ID 或 Mac 登录密码认证后，才会读取 Keychain 中的助记词。支持 secp256k1 和 SLIP-0010 Ed25519。

需要 macOS 14+、Node.js 22+，以及单独构建并正确签名的 App。返回的私钥由调用进程控制，不要记录或持久化。

## 安装

**当前尚未发布。** 正式发布后，在业务项目中运行：

```sh
npm install keylet
```

发布包已包含 JavaScript、类型声明和预编译的原生模块，使用者无需自行编译 SDK。当前从源码开发的步骤见[开发与发布](#开发与发布)。

`keylet` 包不包含 macOS App；请单独安装并打开 Bundle ID 为 `com.nivek3.keylet` 的完整已签名 App，让 macOS 登记其位置。自行构建 App 时，按[构建与签名说明](../README_ZH.md#从源码开始)准备，签名 profile 必须授权此 ID。

## 使用方式

先在 Keylet 中创建或导入钱包。下面的 `trading` 必须替换成 App 中已有的名称，SDK 不会自动创建钱包。

```ts
import { getKeyPair } from 'keylet';

const { publicKey, privateKey } = await getKeyPair({
  name: 'trading',
  curve: 'secp256k1',
  index: 0,
});
```

`curve` 必传。secp256k1 可传可选 `index`（默认 0）或完整 EVM `path`，两者不能同时使用。Ed25519 必须传各级 hardened 的完整路径，如 `{ name: 'trading', curve: 'ed25519', path: "m/44'/501'/0'/0'" }`，不接受 `index`。恢复现有钱包必须使用原钱包的确切派生规则。

结果均为 `0x` hex。secp256k1 返回 32 字节私钥标量和 33 字节压缩公钥；Ed25519 返回 32 字节子 seed 和 32 字节公钥。不要记录或持久化私钥，业务自行管理签名库。

桥接通过 macOS Launch Services 查询 Bundle ID **`com.nivek3.keylet`**，选择系统优先的已登记副本，并从应用元数据读取主程序位置。App 可以改名或移动，但 macOS 需要登记其当前位置。找不到应用时返回 `KEYLET_APP_NOT_FOUND`；选中的应用包或可执行文件无效时返回 `HELPER_START_FAILED`，不会静默改用其他副本。

SDK 始终通过此 Bundle ID 查找 App，不提供路径覆盖参数，传入未知的钱包选择字段会被拒绝。Bundle ID 匹配用于定位候选应用；SDK 暂未在启动前校验发布者签名身份，应使用可信安装。npm 包不包含应用，App 与原生模块应一起构建。包为 ESM，CommonJS 可用 `await import('keylet')`。

每个 Node 进程通过独立私有管道复用自己的 macOS 认证上下文。默认有效期为 300 秒，从认证成功开始固定计时，连续调用不会续期。不缓存密钥或失败结果，每次调用仍重新读取 Keychain。只能从主线程调用，不同 Node 进程与 App 界面操作分别认证。

在 App 的 **… → Settings → Node authentication** 中输入非负整数分钟并保存，默认 5 分钟，`0` 表示每次认证。不设业务上限，数值需能转换为无符号 64 位整数秒。设置从下一次 Node 调用生效，包括已有会话。App 内显示助记词和删除钱包始终重新认证。

可选参数 `authTTL` 指定更短的有效期，单位秒，接受非负 JavaScript 安全整数；不传则跟随 App 设置，`0` 表示每次认证。App 设置为上限，调用方不能覆盖。修改参数会启动新的辅助进程会话。

```ts
const keys = await getKeyPair({
  name: 'trading',
  curve: 'secp256k1',
  authTTL: 0,
});
```

Node 进程退出后会话失效。失败会终止辅助进程，后续显式调用可以重试。删除钱包或锁屏不会撤回已经返回给调用方的密钥。

## 运行示例

先构建 SDK，示例通过 `keylet` 公开入口加载构建产物。从仓库的 `node/` 目录执行，`trading` 是 App 中已保存的钱包名称：

```sh
node --import tsx examples/ethers-v5.ts trading 2
node --import tsx examples/ethers-v6.ts trading 2
node --import tsx examples/viem.ts trading 2
node --import tsx examples/ed25519.ts trading "m/44'/501'/2'/0'"
```

前三个示例只输出 Ethereum 地址；Ed25519 示例只输出 hex 公钥。`index: 2` 对应 `m/44'/60'/0'/0/2`。Ed25519 的路径由调用方选择，不能把其 seed 传给 ethers `Wallet` 当作对应的 Solana 账户。

支持目标是 Node.js 22+。Bun 的 Node-API 支持可能运行这些示例，但本项目尚未完成 Bun 兼容性验证，不能仅因一次调用成功就视为完整支持。

## 故障排查

| 错误 | 检查项 |
| --- | --- |
| `NOT_FOUND` | 名称必须与 App 中的钱包一致；示例不会创建 `trading` 或 `demo` |
| `KEYLET_APP_NOT_FOUND` | 安装并打开 Bundle ID 为 `com.nivek3.keylet` 的 App；npm 包不包含 App |
| `KEYLET_BRIDGE_NOT_BUILT` | 检查原生模块是否构建、架构是否正确，原始加载错误位于 `cause` |
| `SIGNING_CONFIGURATION` | 按签名说明检查证书、profile、bundle ID 和私有访问组 |
| `SETTINGS_ERROR` | 打开 App 的 Settings，重新保存认证选项 |
| `Invalid wallet selection` | curve 必填；path 与 index 互斥；Ed25519 必须指定 hardened-only path |
| `CANCELLED` / `AUTHENTICATION_FAILED` | 在本机完成认证；后续显式调用可重试 |

请求失败后不会自动重试，修复原因后再调用。不要将私钥、助记词或附加口令附到报错中。

## 开发与发布

在源码仓库根目录运行：

```sh
pnpm --dir node install --frozen-lockfile --ignore-scripts
pnpm --dir node run build
pnpm --dir node run check
pnpm --dir node test
```

`build` 构建当前 Mac 架构。构建脚本位于根目录 `scripts/build-bridge.sh`；pnpm 管理开发依赖，npm 负责发布，业务项目可使用自己的包管理器。

发布前安装两个 Rust 目标，然后在 `node/` 打包：

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cd node
npm pack
```

`prepack` 自动重新构建两个架构、合并并检查原生模块，然后生成 JS 和类型声明。任一步骤失败都会停止打包。包内只包含这三个产物、说明和包元数据，没有 Rust 源码、构建脚本或签名资料。

先把生成的 `keylet-0.1.0.tgz` 安装到一个独立项目，验证加载和接口；确认发布账号拥有包名后，使用 `npm publish ./keylet-0.1.0.tgz` 发布同一个已检查的文件。每次发布前更新 `package.json` 的版本号。签名 App 由仓库根目录单独构建。

### GitHub Actions 手动发布

使用 [`.github/workflows/release-node.yml`](../.github/workflows/release-node.yml)，入口为 **Actions → Release Node.js → Run workflow**：

1. 提交发布改动并推送与 `node/package.json` 版本一致的 `vMAJOR.MINOR.PATCH` tag。工作流需先进入默认分支，才会出现在 Actions 中。
2. 填入已有 tag。保留 `draft` 勾选时只构建并提供 `.tgz` 下载，不发布 npm，也不创建 GitHub Release；此模式不需要 npm 或 Apple 凭据。
3. 准备正式发布时，配置仓库 Secret `NPM_TOKEN`：使用具有 `keylet` 发布权限、允许 CI bypass 2FA 的 granular access token，见 [npm 官方 CI 说明](https://docs.npmjs.com/using-private-packages-in-a-ci-cd-workflow/)。先确认账号拥有包名，且包的发布策略允许 token 发布。
4. 再次填入相同 tag，取消 `draft` 后运行。流程会重新构建并验证 tarball，上传到工作流产物，再将本次验证的同一个文件发布到 npm，dist-tag 为 `latest`。已有 npm 版本不能覆盖；下次发布需升级版本并创建新 Git tag。

CI 复用 `prepack` 构建 Universal 原生模块、JS 和类型声明，严格检查包内文件清单，在源码目录之外安装 tarball，并检查原生模块加载和非法输入拒绝，不访问钱包。两个 Mach-O 架构均有检查，但加载检查只在 runner 当前架构执行，不能代替 Intel 运行验证或 App 交互验证。构建和打包脚本不会获得 npm token。App 与 npm 包分别使用 `release-app.yml` 和 `release-node.yml`，各自手动触发并校验对应包的版本。

## 许可证

[MIT](LICENSE)
