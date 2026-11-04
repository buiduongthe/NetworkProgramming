"""Bài 7.8 – Bảng theo dõi đăng ký bộ lọc có ký tự đại diện (Mã giả 7.9, PROCEDURE DASHBOARD; Mục 7.5.5).

Đăng ký nhamay/+/nhietdo (QoS 0, ON_TEMP) và nhamay/+/trangthai (QoS 1, ON_STATUS), rồi chạy vòng sự kiện;
gửi PINGREQ khi im lặng quá nửa thời gian keep alive.

Chạy:  python3 dashboard.py [host] [port] [keep_alive_s] [client_id]   (mặc định localhost 1883 60 dashboard-01)
       Hai client cùng client_id thì broker ngắt client cũ, nên bảng thứ hai cần client_id khác.
"""
import sys

from mqtt_mini import MqttClient


def on_temp(topic, payload, qos, retain):
    print(f"[dashboard] Nhiệt độ  {topic}: {payload}", flush=True)


def on_status(topic, payload, qos, retain):
    print(f"[dashboard] Trạng thái {topic}: {payload} (QoS {qos}{', tin giữ lại' if retain else ''})", flush=True)


def main():
    host = sys.argv[1] if len(sys.argv) >= 2 else "localhost"
    port = int(sys.argv[2]) if len(sys.argv) >= 3 else 1883
    keep_alive = int(sys.argv[3]) if len(sys.argv) >= 4 else 60
    client_id = sys.argv[4] if len(sys.argv) >= 5 else "dashboard-01"
    c = MqttClient()
    c.connect(host, port, client_id=client_id, keep_alive=keep_alive)
    print(f"[dashboard] Đã kết nối broker {host}:{port} là '{client_id}' (MQTT 5.0, keep alive {keep_alive} s)", flush=True)
    for flt, qos, fn in (("nhamay/+/nhietdo", 0, on_temp), ("nhamay/+/trangthai", 1, on_status)):
        granted = c.subscribe(flt, qos, fn)
        print(f"[dashboard] SUBSCRIBE {flt} (QoS {qos}) -> SUBACK, QoS được cấp {granted}", flush=True)
    try:
        c.run_event_loop(on_ping=lambda: print("[dashboard] PINGREQ -> PINGRESP", flush=True))
    except KeyboardInterrupt:
        c.disconnect()
        print("[dashboard] Đã gửi DISCONNECT", flush=True)
    except ConnectionError as e:
        print(f"[dashboard] {e}", flush=True)


if __name__ == "__main__":
    main()
