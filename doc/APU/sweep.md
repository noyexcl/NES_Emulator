Sweep はパルスチャンネルの周期を上下に調整できる

Sweep は以下の部品で構成されている
- Divider
- Reload flag 

# 目標の周期を計算する

Sweep ユニットは連続的に各パルスチャンネルの目標の周期を以下の方法で計算する

1. バレルシフターがパルスチャンネルの11bitのタイマー周期を右にシフトカウントの分だけずらし、変化量を生成する
2. もし Negate flag がセットされていれば、変化量はネガティブになる
3. 目標の周期は、現在の周期と変化量の合計であり、もし合計がマイナスなら0まで丸め込まれる

例えば、Negate flag が false であり、シフト量が0の場合、変化量は現在の周期と等しいので、目標の周期は現在の周期の2倍となる

2つのパルスチャンネルは加算器のキャリー入力をそれぞれ異なって配線しているので、変化量をマイナスにする時に違いが生まれる

1. Pulse1 は1の補数(-c -1)を加算するので、20をマイナスにすると変化量は-21となる
2. Pulse2 は2の補数(-c)を加算するので、20をマイナスにすると変化量は-20となる

$400Xへの書き込みや、Sweep が周期を更新するなど、現在の周期や Sweep の設定が変わる時はいつでも目標周期も変わる


# ミュート

ミュートはパルスチャンネルがミキサーに現在のボリュームの代わりに0を送るということである

ミュートは Sweep ユニットが無効になっているかや Sweep divider がクロック信号を送信しているかに関わらず発生する

Sweep は以下の2つの条件によってミュートになる

1. 現在の周期が8未満
2. どの時点であっても、目標周期が $7FFF より大きい
 
特に Negate flag が false であり、シフトカウントが0で、現在の周期が最小でも $400 の場合、目標周期はミュートに至るまで大きくなる

目標周期は連続的に計算されるので、Sweep の加算器から生じた目標周期のオーバーフローは、Sweep ユニットが無効になっていたり、
Sweep divider がクロック信号を送っていなかったとしてもチャンネルをミュートにする

従って完全に Sweep ユニットを無効にしたい場合、Negate flag をオンにして目標周期が $7FFFより大きくならないようにしなくてはいけない


# 周期の更新

フレームカウンターが Half-frame クロックを送ってきた時、以下の2つのことが起こる

1. Divider のカウントが0であり、Sweep が有効、シフトカウントが0ではない
    1. かつSweep ユニットがチャンネルをミュートしてない: パルスの周期は目標周期に設定される
    2. Sweep ユニットがチャンネルをミュートしている: パルスの周期は変更されないが、Sweep ユニットの divider はカウントダウンを続け、通常と同じようにリロードも行う
2. Divider のカウントが0かReload flag が true: Divider のカウントはPに設定され、Reload flag はクリアされる。そうでない場合は、Divider はデクリメントされる

Sweep ユニットが、シフトカウントが0の時も含め、無効になっている場合、パルスチャンネルの周期は更新されない。しかしミュートのロジックは引き続き適用される
