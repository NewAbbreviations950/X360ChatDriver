# Xbox 360 Wireless Receiver protocol notes

Verified against a real receiver (045e:0719) with `x360chatpad-dump`; raw logs are in `captures/`.
Items marked *(xpad)* come from the kernel `xpad` driver and are not independently re-verified yet.

## USB layout
- 8 interfaces, all interrupt endpoints. Controller slot N (0–3):
  - interface `2N` (protocol 0x81 / 129): gamepad + chatpad data, EP IN `0x81+2N`, EP OUT `0x01+2N`.
  - interface `2N+1` (protocol 0x82 / 130): headset audio, EP IN `0x82+2N`, EP OUT `0x02+2N` (format unknown).
- IN packets are 29 bytes except presence packets (2 bytes).

## Host → controller (OUT, 12 bytes, zero padded)
| Packet | Meaning |
|---|---|
| `08 00 0f c0 00…` | presence inquiry; receiver answers with a presence packet *(xpad)* |
| `00 00 08 4X 00…` | ring LED, `X` = xpad LED command (`6` = player 1 … `9` = player 4) *(xpad)* |
| `00 00 08 c0 00…` | power the controller off |
| `00 01 0f c0 00 SS WW 00…` | rumble, SS = strong (left) motor, WW = weak (right) motor *(xpad)* |
| `00 00 0c 1b 00…` | chatpad: answer to a wake request (`f0 03`) — **send once per wake packet** |
| `00 00 0c 1e` / `00 00 0c 1f` | chatpad keepalive, alternated about once per second |
| `00 00 0c 08`–`0b` | chatpad indicator **on**: Caps, Green, Orange, People |
| `00 00 0c 00`–`03` | chatpad indicator **off**: Caps, Green, Orange, People |

## Controller → host (IN)
### Presence (2 bytes)
`08 80` = controller connected on this slot, `08 00` = slot empty.

### Link info
`00 0f 00 f0 …` sent after connect. Bytes 7–13 look like the controller serial; byte 17 carries the battery code (see *Battery*).

### Gamepad (`00 01 00 f0 …`)
Bytes from offset 4 match the wired 360 report *(xpad)*:
| Byte | Content |
|---|---|
| 6 | dpad up `01`, down `02`, left `04`, right `08`, start `10`, back `20`, LS `40`, RS `80` |
| 7 | LB `01`, RB `02`, guide `04`, A `10`, B `20`, X `40`, Y `80` |
| 8, 9 | LT, RT (0–255) |
| 10–17 | LX, LY, RX, RY as little-endian i16 (Y up = positive) |

### Chatpad (`00 02 00 f0 …`), payload at bytes 24–28
| Bytes 24–25 | Meaning |
|---|---|
| `f0 03` | chatpad wake request → host must reply `0c 1b` (seen ~4× in a burst, answer each) |
| `f0 04` | chatpad active (sent alongside keepalives); byte 26 = indicator state, see below |
| `00 MM` | key report: `MM` = modifier bits, byte 26 = key 1, byte 27 = key 2 (`00` = none) |

Byte 27/28 of status packets is a 4-bit counter that advances with each keepalive.
Without the `1b` handshake the chatpad stays in `f0 03`, its backlight stays off, and no keys are reported.

Modifier bits (byte 25): Shift `01`, Green `02`, Orange `04`, People `08`. They are reported while held.

Key codes (byte 26/27). ✔ = seen in a capture; the rest come from the wired-chatpad table in xboxdrv, which matched every key we verified.
```
1 17✔  2 16  3 15  4 14  5 13  6 12  7 11  8 67  9 66  0 65
Q 27✔  W 26✔ E 25✔ R 24  T 23  Y 22  U 21  I 76  O 75  P 64
A 37✔  S 36  D 35  F 34  G 33  H 32  J 31  K 77  L 72  , 62
Z 46   X 45  C 44  V 43  B 42  N 41  M 52  . 53  Enter 63✔
Left 55  Space 54✔  Right 51  Backspace 71✔
```

### Chatpad indicators
Byte 26 of `f0 04` status packets: Caps `20`, Green `08`, Orange `10`, People `01`,
backlight/awake `80`.
- About 11 s after the last activity, the chatpad goes idle: backlight off, and byte 26 reads `00`.
  It **remembers** the indicators; a keypress wakes it and they reappear.
- An indicator command sent while idle only wakes the chatpad. The chatpad sends `f0 03`,
  we answer `1b`, and the command itself is dropped. Re-send it once byte 26 has `80` set.

### Battery
Two packets carry a battery code. The top three bits of the byte are the code (`code = byte >> 5`); the low five bits stay fixed for each packet type, so the codes are easy to read off.

| Packet | Byte | Fixed low bits | When it's sent |
|---|---|---|---|
| Link info `00 0f 00 f0 …` | 17 | `00011` | on connect (a presence inquiry gets it back, but the value doesn't change while connected) |
| Status `00 00 00 13 XX` (and `00 00 00 17 XX`) | 4 | `00010` | sporadically while connected |

Observed codes, from resets at each voltage (sweep 2, 3.0 V down to 2.1 V):

| Code | Byte values | Voltage | Flashing | Bars (shown in the popup) |
|---|---|---|---|---|
| 7 | `e3` (link), `e2` (status) | 2.7–3.0 V | no | full |
| 5 | `a3`, `a2` | 2.38–2.6 V | no | medium |
| 3 | `63`, `62` | 2.31–2.37 V | yes | low |
| 1 | `23`, `22` | 2.1–2.31 V | yes | empty |

- The code boundaries are about 2.37–2.38 V (5 → 3, the low-battery flash starts), about 2.6–2.7 V (7 → 5), and about 2.30–2.31 V (3 → 1). At 2.31 V the two status packets were split between codes 1 and 3, so the boundary is noisy.
- The low-battery flash rate changes between 2.1 and 2.3 V while the code stays at 1. Flash rate isn't in these packets, so it can't be read from the protocol.
- The first sweep (falling, no resets) gave values that don't fit this table: `c0` at 2.35 V, `3c` at 2.4–2.45 V, and code 1 at 2.4 V. Pad history or hysteresis probably explains them, but that isn't shown yet.
- Codes are only refreshed on connect (link) or sporadically (status), so a live bar would lag. A 4-bar display should treat the link byte as a connect-time snapshot.
- Not yet explained: `c0` and `3c` (low bits don't match the table), and the exact full/medium boundary between 2.6 and 2.7 V.
- SDL's HIDAPI Xbox 360 wireless driver reads these same bytes (link byte 17, status byte 4) and sends no battery request. It maps the value to four buckets, but its 0–255 thresholds don't match the codes we see. [SDL source](https://github.com/libsdl-org/SDL/blob/a8780c6a287bb97549ec4411cff7ade560db441f/src/joystick/hidapi/SDL_hidapi_xbox360w.c)

### Unknown
- `00 f8 0N 00…` (N = 1–3): seen only while the chatpad was not handshaked and keys were being pressed. Possibly "key pressed while asleep" or an attachment status.
- `00 f8 01` / `00 f8 02` alternate every few seconds while connected; meaning unknown.
- `00 00 00 f0` (every few seconds while connected) and `00 00 00 20/30/40/60` (a burst after connect); meaning unknown.
