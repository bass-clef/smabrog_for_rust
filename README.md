
smabrog
===

## Description
大乱闘スマッシュブラザーズSpecial (super smash brothers special) の為の自動戦績保存/送信ツール

smabrog の Rust 移植/改変版です。

## Usage
- Download
    - 最新の smabrog.zip は [ここ](https://github.com/bass-clef/smabrog_for_rust/releases/)

- 忙しい人向け
    - DL して解凍 smabrog_installer.exe で自分のユーザーのみにインスコ、起動。
    - スイッチ、スマブラ、キャプチャソフトをつけて ```ReadyToFight``` をキャプチャして Go!

- 導入の方法

    0. 必要なもの
        - Switchおよび大乱闘スマッシュブラザーズSpecial
        - PCおよび任意のキャプチャソフト
        - smabrog_installer.exe
    1. キャプチャソフト と スマブラSP を起動して オンラインの [READY to FIGHT]が表示された画面にします
    2. 最新のリリースから smabrog.zip ダウンロードして、解凍して smabrog_installer.exe を実行する
        - ```すべてのユーザーにインストールするとシステムドライブに展開されるので、実行時に管理者権限が必要になります。```
        - 指示に従って同梱してある MongoDB もインストールします。
    3. 起動したら Capture Mode を任意に選択して Apply を押すと、自動でキャプチャ画面を捕捉します。
        - 誤検出されないように他のウィンドウを最小化または閉じておく事をおすすめします。
        - リソースの解像度と一致するため ```640 x 360``` の解像度が一番正確に検出できます
        - [From Desktop] で検出したあとに、キャプチャソフトを移動すると検出できなくなります。
        - キャプチャソフト OBS の方向け
            - [From VideoDevice] に OBS の仮想ビデオデバイスが表示されますが、OpenCV で仮想ビデオデバイスを読み込めないみたいなので、
            - プレビューあたりを右クリックで出てくる [```プロジェクター (プレビュー)```] を [From Window] でキャプチャする事をおすすめします。
    4. READY to FIGHT!

- 戦績を参照する
    - smabrog.exe から見る
        - 単に起動すると過去 10 件分の戦歴が閲覧できます。
    - MongoDB からソースを見る
        - 同時にインストールした MongoDB Compass を起動。
        - [mongodb://localhost] を入力して接続。
        - smabrog-db / battle_data_col に戦歴データが入ってるのでご自由にしてください。
        - 自分のサーバーに送信したいという方がいる場合は作者にTwitterDMなりで連絡をとってみて下さい。

- オプション
    - config.json の記述
    ```json
        "window_x":             /* smabrog の前回起動時位置 */
        "window_y":             /* smabrog の前回起動時位置 */
        "capture_win_caption":  /* [From Window] のウィンドウタイトル */
        "capture_win_class":    /* [From Window] のクラス名 */
        "capture_device_name":  /* [From VideoDevice] のデバイス名 */
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

### 動作済み環境
    - Super Smash Bros. SPECIAL(Ultimate) ver 11.*
    - Windows 10 (mem:32.0GB, CPU:i7-8565U [1.80GHz])

### Q&A
- Q. スマブラが検出されない
    - A. 起動してから任意で Apply してもキャプチャソフトの画面を捕捉できない場合場合があります。
        => Q. 検出率を上げるには

- Q. 検出率を上げるには
    - A. 下記の対処法が考えられます
        - キャプチャソフトの解像度が [```16:9```] なのを確認してください。対応解像度: [```640x380```] ~ [```1920x1080```]
        - [READY to FIGHT]の画面がはっきり表示されているのを確認してください。
        - 可能であれば一度 キャプチャソフト や smabrog.exe 以外のソフトを起動していない状態でご確認下さい。
            - [From Window] 別のウィンドウが補足されている可能性があります。
            - [From Desktop] 別の領域が誤検出されている可能性があります。
    - 何かが検出された場合は インストールディレクトリに [```found_capture_area.png```] が作成されるので、一度ご確認下さい。
        - 赤い枠内、四辺に 1px ずつの隙間があります。
- Q. 試合結果がうまく検出されない
    - [Job: Busy] 状態だと CPU 使用率が高くなっているので **自分の順位や戦闘力が表示されてる画面** をいつもよりゆっくり進んでいくとより検出できるようになります
    - ウィンドウの移動などでキャプチャソフトが補足できていない可能性があります。
    - 何かが検出された場合は インストールディレクトリに [```temp.avi```] が作成されるので、変な所が検出されていた場合、
    
        お手数ですが、ログと一緒に作者に送りつけてあげてください。

### 既知のバグ
- [From Desktop]でキャプチャした瞬間が重たい
- ROY(KOOPA Jr.) / ROY が判別できない。デフォルトでは 剣士のほうの ROY になります。
- キャラクターの誤検出
    - Dr.Mario などの [```.```] が含まれている名前や、Mii系の複数行で記述されている名前で誤検出がよく起こります。
- [From Window] の input 内で日本語が文字化けする

## Author/Licence
- [Humi@bass_clef_](https://twitter.com/bass_clef_)
- [MIT License](https://github.com/bass-clef/smabrog_for_rust/src/LICENSE)
- [Tesseract-OCR](https://github.com/tesseract-ocr/tesseract#license)
- [大乱闘スマッシュブラザーズ SPECIAL](https://www.smashbros.com/)  
    smabrog に使用しているゲーム内画像の著作権、商標権その他の知的財産権は、当該コンテンツの提供元に帰属します

## Special Thanks
- カービィを使って youtube に動画を上げてくれた方々、デバッグで大変お世話になりました！ありがとうございます！

## log
- 2021/6/11
    0.19 を公開しました