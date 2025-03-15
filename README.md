# rktk-keyball-rs

拙作の[rktk](https://github.com/nazo6/rktk)というフレームワークを用いたRust製のKeyballファームウェアです。現在Keyball61のみをサポートしています。

動作のためにはRP2040を搭載したProMicroが必要です。AliExpressなどで互換品がお安く買えます。通常のAVR
ProMicroでは動かないので注意してください。

また、BLEに対応しておりnRF52840を搭載ボードでも動作しますが、BLE Micro
Proでの動作は現状確認していません。
ピンの設定を適切に変更すれば動作するはずですが、本ファームウェアでは過去フラッシュの書き込みにバグがあり書き換えてはいけない領域を書き換えてブートローダが起動しなくなることがあったため自己責任でお願いします。

## 昔のコード

[Zennの記事](https://zenn.dev/nazo6/articles/keyball-embassy-rp2040)で紹介した際の`keyball-rs`のコードは[`legacy`ブランチ](https://github.com/nazo6/keyball-rs/tree/legacy)にあります。
このコードをライブラリ化したものがrktkです。

## 機能

詳しくは[rktkのページ](https://github.com/nazo6/rktk)を参照してください。キーマップについてはQMKの機能のメジャーな所に相当するものは大体実装してありますが、ディスプレイ、バックライトは現状カスタマイズすることができません。

## 既知の不具合

- 左右間の通信が安定しない

## ビルド(RP2040)

### 依存

ビルドには以下のツールが必要です。予めインストールしておいてください。

- Nightly Rust: Rustupからインストール可能
- [flip-link](https://github.com/knurling-rs/flip-link):
  `cargo install flip-link`
- [rktk-cli](https://github.com/nazo6/rktk):
  `cargo +nightly install --git https://github.com/nazo6/rktk rktk-cli`
- [cargo-binutils](https://github.com/rust-embedded/cargo-binutils)
  ```
  cargo install cargo-binutils
  rustup component add llvm-tools
  ```

### 手順

1. このリポジトリをクローンします。
   ```bash
   git clone https://github.com/nazo6/keyball-rs
   ```

2. ビルドするディレクトリに移動してビルドします。
   ```bash
   cd keyball-rs/keyball61/keyball61-rp2040
   rktk-cli build
   ```

3. ビルドが完了すると`target/thumbv6m-none-eabi/min-size`にuf2ファイルが生成されているはずです。ProMicroをブートローダーモードで起動(BOOTを押しながらリセット)し、表れたドライブにuf2ファイルをコピーしてフラッシュしてください。

## カスタマイズ

### キーマップ

キーマップは[keymap.rs](./keyball-common/src/keymap.rs)で定義されています。これを編集することでキーマップを変更することができます。

### Remapper

rktkではソースコードでキーを変更する以外にも、以下のWebアプリを使うことでキーマップや設定を変更することができます。

https://rktk-client.nazo6.dev/
