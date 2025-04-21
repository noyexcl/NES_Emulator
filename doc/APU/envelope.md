シンセサイザーにおいてエンベロープは音のパラメーターを時間の経過と共に変化させる方法である

NESのAPUは、2つのうちいずれかの方法でボリュームを調整するエンベロープジェネレーターを持っている

1. 適切なループと共にノコギリ減衰型のエンベロープを生成する
2. より洗練されたエンベロープジェネレーターが操作できる一定の音量を生成する

各エンベロープは以下のような部品を含み、構成される

- Start flag
- Divider
- Decay level counter

# 挙動

フレームカウンターによってクロックされた時、2つのアクションのうちいずれかが起こる

もし Start flag がクリアされていれば、Divider がクロックされる

そうでなければ、Start flag はクリアされ、Decay level counter が15でロードされる。そして Divider の期限が即座にリロードされる

Divider が0でクロックされると対応するレジスターで設定された値 V でロードされ、Decay level counter をクロックする

そして2つのアクションのうちいずれかが起こる

もしカウンターが0でないなら、デクリメントされる

カウンターが0であり、ループフラグがセットされていれば、Decay level counterが15でロードされる

エンベロープのボリューム出力は Constant volume flag による

セットされていれば、エンベロープパラメータが直接ボリュームに設定される

クリアされていれば、Decay level が現在のボリュームになる

Constant volume flag はボリュームの元を設定する他、何の効果も持たない。Decay level は Constant Volume が選択されている間も更新され続ける

エンベロープの出力は、以下の追加のゲート Sweep (Pulseの時のみ)、Waveform generator(sequencer or LFSR)、length counter を通して供給される
