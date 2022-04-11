﻿
smabrog
===

## Description
大乱闘スマッシュブラザーズSpecial (super smash brothers special) の為の自動戦績保存/送信/表示ツール

smabrog の Rust 移植/改変版です。

- スクリーンショット  
    ![50戦の戦歴と外観](./thumbnails_0.png) ![対キャラ表とカスタマイズ](./thumbnails_1.png) ![対キャラ検索と詳細設定](./thumbnails_2.png)

## Usage
- Download
    - 最新の smabrog_installer.exe は [ここ](https://github.com/bass-clef/smabrog_for_rust/releases/)

- 忙しい人向け
    - DL して解凍 smabrog_installer.exe で自分のユーザーのみにインスコ、起動。
    - スイッチ、スマブラ、キャプチャソフトをつけて ```ReadyToFight``` をキャプチャして Go!

- 動画  
    [![【もう楽ちん】スマブラSPで連勝、キャラ表とかを自動で表示するツール作った](https://img.youtube.com/vi/Qrx-SrqpIhE/0.jpg)](https://www.youtube.com/watch?v=Qrx-SrqpIhE)

- 導入の方法
    0. 必要なもの
        - Switchおよび大乱闘スマッシュブラザーズSpecial
        - PCおよび任意のキャプチャソフト
        - smabrog_installer.exe
    1. キャプチャソフト と スマブラSP を起動して オンラインの [READY to FIGHT]が表示された画面にします
    2. 最新のリリースから smabrog.zip ダウンロードして、解凍して smabrog_installer.exe を実行する
        - ```すべてのユーザーにインストールするとシステムドライブに展開されるので、実行時に管理者権限が必要になるので、１ユーザーにインストールをオススメします```
        - 指示に従って同梱してある MongoDB もインストールします。
    3. 起動したら、黒い画面は最小化なりをしても構わないです。本体の方の３つ目のウィンドウのソースの種類を、自身の環境にあったものに選択します、すると自動でキャプチャ画面を捕捉します。
        - このときに予めスマブラの方で「ReadyToFight」が表示された画面にしておく必要があります。
        - デスクトップから検出する場合は誤検出されないように他のウィンドウを最小化または閉じておく事をおすすめします。
        - リソースの解像度と一致するため ```640 x 360``` の解像度以上が一番正確に検出できます。(他解像度だと数字などが潰れてうまく検出できない可能性があります)
        - ソース/デスクトップ で検出したあとに、キャプチャソフトのウィンドウを移動すると検出できなくなります。
        - キャプチャソフト OBS の方向け
            - ソース/ビデオデバイス で OBS の仮想ビデオデバイスが選択できるようになってますが、OpenCV で仮想ビデオデバイスを読み込めないみたいなので、
            - プレビューあたりを右クリックで出てくる [```プロジェクター (プレビュー)```] を ソース/ウィンドウ でキャプチャする事をおすすめします。(ある程度の大きさにリサイズしてください)(全画面版でも可)
    4. READY to FIGHT!

- 戦績を参照する
    - smabrog.exe から見る
        - 単に起動すると過去 N 件分の戦歴が閲覧できます。(設定/詳細から取得件数を変更できます)
    - MongoDB からソースを見る
        - 同時にインストールした MongoDB Compass を起動。
        - [mongodb://localhost] を入力して接続。
        - smabrog-db / battle_data_col に戦歴データが入ってるのでご自由にしてください。
        - 自分のサーバーなどに送信したいという方がいる場合は[作者](https://twitter.com/bass_clef_)にTwitterDMなりで連絡をとってみて下さい。

- オプション
    - 設定/詳細  
        - 結果取得限界          - N 戦の戦歴に使用されます。連勝記録もこの数値が限界値となってます。
        - 無効化 音量           - 無効化されている BGM を検出した際に、下記のコンボボックスで選択されている オーディオデバイス/プロセス の音量を指定の音量に変更します。
        - 再生リスト            - 無効化されている BGM を検出した際に、指定の再生リストのフォルダからランダムに再生します。
        - 残基 警告             - 指定の数未満のストックを検出すると、指定のファイルを再生します
    
    - config.json の記述
    ```json
        "window_x":             /* smabrog の前回起動時位置 */
        "window_y":             /* smabrog の前回起動時位置 */
        "capture_win_caption":  /* ソース/ウィンドウ のウィンドウタイトル */
        "capture_win_class":    /* [From Window] のクラス名 */
        "capture_device_name":  /* [From VideoDevice] のデバイス名 */
        "result_max":           /* 結果取得数 */
        "lang":                 /* GUIの表示言語 */
        "visuals":              /* GUIに関するデータ */
    ```

    - database の構造
    ```json
        id                      /* Database Object Id */
        start_time              /* 3/2/1/Go! の開始時刻が入ってます */
        end_time                /* GameSet/TimeUp いずれも試合終了時刻が入ってます */
        player_count            /* プレイヤーの人数 */
        rule_name               /* Time/Stock/Stamina のいずれか、未検出の場合は Unknown が入ってます */

        /*
            Time   : 時間制限あり[2,2:30,3], ストック数は上限なしの昇順, HPはバースト毎に0%に初期化
            Stock  : 時間制限あり[3,4,5,6,7], ストック数は上限[1,2,3]の降順, HPはバースト毎に0%に初期化
            Stamina: 時間制限あり[3,4,5,6,7], ストック数は上限[1,2,3]の降順, HPは上限[100,150,200,250,300]の降順
        */
        max_time                /* ham vs spam で検出した時間 */
        max_stock_list          /* ham vs spam で検出したストック数(チーム戦の時、優先ルール ON で相手チームとストック数が違う場合にお互いのストックを分け合うので将来用[未定義]) */
        max_hp_list             /* [-1] * プレイヤー数 (ham vs spam で検出したHP[未定義]) */

        chara_list              /* キャラクターのリスト 1pから順に入ってます */
        group_list              /* チーム戦の場合のチームカラー */
        stock_list              /* 最終的な残機 / (消費したストック数[未定義]) */
        order_list              /* 順位 (team戦の場合は 同順が入ってくる事に注意) */
        power_list              /* 戦闘力 */
    ```

### Q&A
- Q. インストールしたのに起動されない  
    - A. 管理者権限が必要なインストールをした場合は、起動するときに右クリックから「管理者として実行(A)」で起動してください  
    - OpenGL, GPU に対応していない環境だとエラーすら表示されずに終了します(内部GPUでも一応動作確認済みです)  

- Q. スマブラが検出されない
    - A. 起動してから任意の方法でキャプチャしても ReadyToFight を捕捉できない場合場合があります。
        => Q. 検出率を上げるには

- Q. 検出率を上げるには
    - A. 下記の対処法が考えられます
        - キャプチャソフトの解像度が [```16:9```] なのを確認してください。対応解像度: [```640x360```] ~ [```1920x1080```]
        - [READY to FIGHT]の画面がはっきり表示されているのを確認してください。  
        - 可能であれば一度 キャプチャソフト や smabrog.exe 以外のソフトを起動していない状態でご確認下さい。  
            - ソース/ウィンドウ: 別のウィンドウが補足されている可能性があります。  
            - ソース/デスクトップ: 別の領域が誤検出されている可能性があります。  
    - 何かが検出された場合は インストールディレクトリに [```found_capture_area.png```] が作成されるので、一度ご確認下さい。
        - 赤い枠内、四辺に 1px ずつの隙間があります。

- Q. 試合結果がうまく検出されない
    - 動画を常に検出しているような状態なので、CPU 使用率が高くなります。 **自分の順位や戦闘力が表示されてる画面** をいつもよりゆっくり進んでいくとより検出できる可能性が上がります。  
    - MongoDB のタイムアウトが何回も出ている場合は再起動で直る場合があります。  
    - ウィンドウの移動などでキャプチャソフトが補足できていない可能性があります。  
    - 何かが検出された場合は インストールディレクトリに [```temp.avi```] が作成されるので、変な所が検出されていた場合、お手数ですが、インストールディレクトリごと [作者](https://twitter.com/bass_clef_) に送りつけてあげてください。

### 既知のバグ
- ソース/デスクトップ でキャプチャした瞬間が重たい
- ROY(KOOPA Jr.) / ROY が判別できない。デフォルトでは 剣士のほうの ROY になります。
- ソース/ビデオデバイス での OBS Virtual Camera でデッドロックになる

### 動作済み環境  
- Super Smash Bros. SPECIAL(Ultimate) ver 13.*  
    - Windows 11 Home (mem:16.0 GB, CPU:i7-8565U [1.80GHz])  
    - Windows 10 Pro (mem:32.0GB, CPU:i7-8565U [1.80GHz])  
    - Windows 10 Home (mem:16.0 GB, CPU:i7-8565U [1.80GHz])  
    - Windows 10 Sandbox  
        - OpenGL が必要なので、対応した環境で下記を *.wsb として保存して起動してください  
        ```xml
        <Configuration>
        <VGpu>Enable</VGpu>
        <Networking>Default</Networking>
        </Configuration>
        ```

### 動作確認できなかった環境  

## Author/Licence
- [Humi@bass_clef_](https://twitter.com/bass_clef_)
- [MIT License](https://github.com/bass-clef/smabrog_for_rust/src/LICENSE)
- [Tesseract-OCR](https://github.com/tesseract-ocr/tesseract#license)
- [大乱闘スマッシュブラザーズ SPECIAL](https://www.smashbros.com/)  
    smabrog に使用している、画像の著作権、商標権その他の知的財産権は、当該コンテンツの提供元に帰属します

## Special Thanks
- カービィを使って youtube に動画を上げてくれた方々、デバッグで大変お世話になりました！ありがとうございます！
- BGM のリストを作成する際に参考になったサイト[「ゲーマー夫婦 みなとも」](https://gamelovebirds-minatomo.link/smashbros-music-list/#_113)

## 支援
- [Amazon ほしいも](https://www.amazon.jp/hz/wishlist/ls/1GT79HREJVH1C?ref_=wl_share)

## log
- 2022/4/  
    ver 0.33.0 を公開しました  
    - 検出が成功したソースを保存し次回起動時に自動で読み込むようにしました  
    - ルールの検出方法を改善しました  
    - ReadyToFight のソースをより鮮明にしてブレを少なくしました  
- 2022/4/7  
    ver 0.32.0 を公開しました  
    - Windows 11 と Windows Sanbbox での動作確認をしました  
    - ソース/ウィンドウ のキャプチャ方法を WinAPI から DirectX 11 に変更しました  
    - smabrog ウィンドウの初期位置を 0x0 に移動しました  
    - トーナメントの試合結果の保存がうまくいかなくなる時があるのを修正しました  
    - 試合開始と終了 の検出を改善しました  
- 2022/3/8  
    ver 0.31.1 を公開しました  
    - 指定の BGM を推測して OFF または別の音楽を流せるようにしました(日本語のみ)  
    - ストックの下限を設けて、それ未満だと指定コマンドを実行できるようにしました  
    - 体力制のHP, 時間制の制限時間 を検出するようにしました  
    - 戦歴を GUI で削除できるようにしました  
    - 文字列の比較のアルゴリズムをゲシュタルトパターンマッチングからジャロ・ウィンクラー距離法に変えました  
    - 現在キャプチャしている画面を確認できるようにしました  
    - ストック/時間/ルール の検出を改善しました  
    - GameEnd の検出ボーダーを下げました  
    - ピクミン＆オリマーを検出していなかったのを修正しました  
- 2022/2/8  
    ver 0.30.0 を公開しました  
    - キャラ表の表示がおかしかったのを修正しました  
    - Maching のボーダーを少し下げて検出しやすくしました  
    - ビデオデバイスの検出状態を出力するようにしました  
    - 状態が正しく表示されるように修正しました  
- 2022/2/7  
    ver 0.29.0 を公開しました  
    - 1on1 のトーナメントを検出できるようにしました  
    - フォントの大きさと種類を変更できるようにしました  
    - 外観をカスタマイズできる機能を追加しました  
    - 細かいバグ修正をしました  
- 2022/2/4  
    ver 0.28.2 を公開しました  
    - キャラクター別の検索/表示機能を作成しました  
    - 結果を 1 から 1000 件まで変更できるようにしました  
    - 連勝数を表示するようにしました  
    - 一定試合するとDBに保存できなくなるバグを修正しました  
    - 1on1での差が大きい世界戦闘力を無視するようにしました  
    - MongoDB が動作不能になるのを修正しました  
- 2022/2/1  
    ver 0.27 を公開しました  
    - 対キャラ表を表示できるようにしました  
    - ボタンで順位を修正できるようにしました  
    - 勝率の隣に試合数を表示するようにしました  
    - mongodb と tesseract-OCR がインストーラーに同梱されていなかったのを修正しました  
    - 細かいバグ修正をしました  
- 2022/1/25  
    ver 0.26 を公開しました  
    - 誤って検出された世界戦闘力をできるだけ適用しないようにしました  
    - 細かいバグ修正をしました  
- 2021/12/29  
    ver 0.25 を公開しました  
    - 英語版SSBUおよび、GUIを英語に対応しました  
    - テーマに Dark と Light を選べるようにしました  
    - 検出率を GUI のほうでも表示するようにしました  
    - Light テーマの時に世界戦闘力の表示を、数値と文字だけに見えるようにしました  
    - リソースを高解像度にして Matching シーンの検出率を上げました  
- 2021/12/24  
    ver 0.24 を公開しました  
    - GUI を変更しました  
    - 世界戦闘力を表示するようにしました  
    - キャラクター別の勝率を表示するようにしました  
- 2021/12/15  
    ver 0.23.1 を公開しました  
    - 試合終了時の順位の検出率を改善しました  
- 2021/12/03  
    ver 0.23 を公開しました  
    - フォントを変更し完全に日本語に対応して、ウィンドウリストを表示するようにしました  
    - 動画判定時の検出色を適切に処理するように改善しました  
        - 試合開始のシーンが検出率を改善しました  
        - 試合終了時の順位の検出率を改善しました  
- 2021/12/02  
    複数行のキャラ名を検出できるようにし、順位の検出率を上げました  
    ver 0.22 を公開しました  
- 2021/10/20  
    ソラ を追加し、0.21 を公開しました
- 2021/7/08  
    カズヤ を追加し、0.20 を公開しました  
- 2021/6/11  
    0.19 を公開しました  
