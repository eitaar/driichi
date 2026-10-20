# Driichi

[English](README.md) | **日本語**

Driichiは、**mjaiプロトコル対応の麻雀AIをテスト・評価するための対戦環境**です。人間、mjai対応AI、MCP経由で接続するLLMエージェント、内蔵Botが同じ卓に参加でき、**人間 vs LLM** や **LLM vs mjai対応AI** といった対局もできます。

## 主な機能

- **異なる種類のプレイヤーによる対局：** ブラウザから参加する人間と、mjai対応AIやMCP経由のエージェントが同じ卓で対局できます。
- **麻雀AIの評価：** Bot専用のベンチマークで、対局メンバーを固定し、席順をローテーションしながら複数対局の結果やリプレイを記録できます。
- **リーチ麻雀：** 3人・4人のルーム対局に対応しています。ルール判定と合法手の管理はサーバー側で行います。
- **エージェント連携：** mjai対応クライアントや、MCP経由のLLMエージェントを接続できます。

> **互換性に関する注意：** 現行のv1では3人麻雀のmjai接続には対応していません。また、riichi.dev互換のmjaiアダプターは外部環境との互換性検証が完了していません。

## はじめに

現時点でGitHub Releasesに配布済みのバイナリはないため、ソースコードからローカル用のリリースパッケージをビルドしてください。

必要なもの：Git、**Rust 1.95以降**、Node.js/npm、Python 3、FFmpeg。リリース対象はLinux（x86_64 / ARM64）とWindows（x86_64）です。

```sh
git clone https://github.com/eitaar/driichi.git
cd driichi
python scripts/release/package.py build --platform auto
```

生成されたアーカイブは `target/release-artifacts/` に保存されます。展開後、以下の手順で起動できます。

1. `config.toml.example` を `config.toml` に、`.env.example` を `.env` にコピーします。
2. `driichi hash-password`（Unixでは `./driichi hash-password`）を実行し、生成されたハッシュを `.env` の `ADMIN_PASSWORD_HASH` に設定します。
3. `.env` に `ADMIN_USERNAME` を設定し、`driichi --config config.toml`（Unixでは `./driichi --config config.toml`）でサーバーを起動します。
4. ブラウザで `http://127.0.0.1:3000` を開きます。

設定ファイルのディレクトリには実行時データや認証情報が保存されます。`.env` やBot Tokenなどの秘密情報をコミットしないでください。

デプロイに関する詳細は[リリースガイド（英語）](release/README.md)を参照してください。

## AI・エージェントの接続

**mjai対応AI：** 外部の麻雀AI向けにmjaiアダプターを提供しており、riichi.dev互換のエンドポイントも用意しています。現在の対応範囲については[実装仕様（英語）](spec/implementation-v1.md)を参照してください。

**LLMエージェント（MCP）：** Bot Tokenで認証し、サーバーの `/mcp` エンドポイントに接続します。オプションの `driichi-mcp` バイナリを使うと、stdio方式のMCPクライアントから接続できます。

```sh
DRIICHI_MCP_TOKEN=<your-bot-token> driichi-mcp --server http://127.0.0.1:3000/mcp
```

ChatGPT向けのOAuthゲートウェイはオプションです。別途、HTTPSで公開した環境での設定・検証が必要であり、デフォルトでは無効です。

**人間：** ブラウザからルームに参加できます。同じルームで、人間・mjai対応AI・MCPエージェント・内蔵Botを対局メンバーとして選択できます。

## ベンチマークとリプレイ

**Benchmark Mode** はBot専用の評価モードです。参加Botを固定したまま席順をローテーションして複数の対局を行い、参加者ごとの結果を比較できます。Benchmark Modeに人間は対局参加できないため、**人間 vs AI** は通常のルームで行ってください。

完了した対局は記録し、リプレイシステムで確認できます。

## 開発

リポジトリのローカルチェックは次のコマンドで実行できます。

```sh
just check
```

主な構成はRust製の対局エンジン・サーバー、mjai / MCPアダプター、リプレイ機能、React / TypeScript製フロントエンドです。設計の詳細は[実装仕様（英語）](spec/implementation-v1.md)と[用語集（英語）](CONTEXT.md)を参照してください。
