"""Bài 7.8 – Thiết bị cảm biến phát số đo nhiệt độ qua MQTT (Mã giả 7.9, PROCEDURE SENSOR; Mục 7.5.5).

Kết nối với di chúc (Last Will) "offline" giữ lại, phát "online" (QoS 1, giữ lại), sau đó phát số đo
JSON {v, ts} QoS 0 mỗi chu kỳ. Khi đủ số lần: tắt đúng thủ tục (phát "offline" rồi DISCONNECT), hoặc
với tùy chọn --mat-ket-noi thì đóng socket đột ngột để broker tự phát di chúc.

Chạy:  python3 sensor.py <mã_thiết_bị> [host] [port] [số_lần] [chu_kỳ_ms] [--mat-ket-noi]
       (mặc định localhost 1883, số_lần 0 = chạy mãi, chu kỳ 5000 ms)
"""
import json
import random
import sys
import time

from mqtt_mini import MqttClient


def read_sensor():
    """READ_SENSOR(): giá trị mô phỏng."""
    return round(random.uniform(24.0, 32.0), 1)


def main():
    args = [a for a in sys.argv[1:] if a != "--mat-ket-noi"]
    lose = "--mat-ket-noi" in sys.argv
    device_id = args[0] if args else "day1"
    host = args[1] if len(args) >= 2 else "localhost"
    port = int(args[2]) if len(args) >= 3 else 1883
    times = int(args[3]) if len(args) >= 4 else 0
    period_ms = int(args[4]) if len(args) >= 5 else 5000
    st = f"nhamay/{device_id}/trangthai"
    c = MqttClient()
    c.connect(host, port, client_id=device_id, keep_alive=30, will=(st, "offline", 1, True))
    print(f"[{device_id}] Đã kết nối broker {host}:{port} (MQTT 5.0, keep alive 30 s, di chúc '{st}' = offline)", flush=True)
    c.publish(st, "online", qos=1, retain=True)
    print(f"[{device_id}] PUBLISH {st} = online (QoS 1, giữ lại) -> đã nhận PUBACK", flush=True)
    n = 0
    while times == 0 or n < times:
        n += 1
        msg = json.dumps({"v": read_sensor(), "ts": int(time.time())}, separators=(",", ":"))
        c.publish(f"nhamay/{device_id}/nhietdo", msg, qos=0)
        print(f"[{device_id}] PUBLISH nhamay/{device_id}/nhietdo = {msg} (QoS 0)", flush=True)
        time.sleep(period_ms / 1000)
    if lose:
        c.sock.close()                                    # mô phỏng mất điện: không gửi DISCONNECT
        print(f"[{device_id}] Mô phỏng mất kết nối: đóng socket, không gửi DISCONNECT", flush=True)
    else:
        c.publish(st, "offline", qos=1, retain=True)
        c.disconnect()
        print(f"[{device_id}] Tắt đúng thủ tục: phát offline rồi gửi DISCONNECT", flush=True)


if __name__ == "__main__":
    main()
