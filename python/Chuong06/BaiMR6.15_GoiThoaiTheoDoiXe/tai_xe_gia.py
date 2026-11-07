"""Bài mở rộng MR6.15 – Tài xế giả lập: chạy theo tuyến mẫu và gửi vị trí qua MQTT (giống chế độ "Giả lập" của app).

Mỗi chu kỳ (--chu-ky giây) xe tiến thêm tốc_độ × chu_kỳ mét dọc tuyến (tuyen_mau.json), thêm nhiễu GPS ngẫu nhiên
(--nhieu mét, phân bố chuẩn) rồi PUBLISH lên xe/<mã xe>/vitri, QoS 0, retain (khách vào sau nhận ngay điểm cuối):
  {"lat","lon","toc_do" km/h,"huong" độ,"do_chinh_xac" m,"nguon":"gia_lap","t" ms,"ten","xe","tuyen",
   "con_lai_m","dich":{"lat","lon","ten"}}
Kết nối có Last Will: xe/<mã xe>/trangthai = "mat_ket_noi" (retain) – broker tự phát khi tài xế mất sóng; vào mạng
phát "truc_tuyen", nghỉ đúng cách phát "nghi". --mat-song: cuối lượt đóng TCP đột ngột để xem Last Will hoạt động.

Chạy:  python tai_xe_gia.py <tên> <mã xe> [host] [cổng_mqtt] [--tuyen 0] [--toc-do 30] [--chu-ky 1] [--nhieu 5]
                            [--bat-dau 0.0] [--thoi-gian 0] [--mat-song]
"""
import argparse
import asyncio
import json
import math
import os
import random
import time

from mqtt_khach import MqttKhach

R_TRAI_DAT = 6371000.0


def khoang_cach(a, b):
    """Khoảng cách haversine (m) giữa hai điểm (lat, lon)."""
    la1, lo1, la2, lo2 = map(math.radians, (a[0], a[1], b[0], b[1]))
    h = math.sin((la2 - la1) / 2) ** 2 + math.cos(la1) * math.cos(la2) * math.sin((lo2 - lo1) / 2) ** 2
    return 2 * R_TRAI_DAT * math.asin(math.sqrt(h))


def huong(a, b):
    """Hướng đi (độ, 0 = bắc, theo chiều kim đồng hồ) từ a tới b."""
    la1, la2, dl = math.radians(a[0]), math.radians(b[0]), math.radians(b[1] - a[1])
    y = math.sin(dl) * math.cos(la2)
    x = math.cos(la1) * math.sin(la2) - math.sin(la1) * math.cos(la2) * math.cos(dl)
    return (math.degrees(math.atan2(y, x)) + 360) % 360


class Tuyen:
    def __init__(self, t):
        self.ten, self.diem_den, self.vong, self.diem = t["ten"], t["diem_den"], t["vong"], t["diem"]
        self.doan = [khoang_cach(self.diem[i], self.diem[i + 1]) for i in range(len(self.diem) - 1)]
        self.dai = sum(self.doan)

    def tai(self, s):
        """Điểm và hướng tại quãng đường s (m) tính từ đầu tuyến."""
        s = s % self.dai if self.vong else min(s, self.dai)
        for i, d in enumerate(self.doan):
            if s <= d or i == len(self.doan) - 1:
                k = s / d if d else 0
                a, b = self.diem[i], self.diem[i + 1]
                return (a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k), huong(a, b)
            s -= d


def doc_tuyen():
    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "tuyen_mau.json"), encoding="utf-8") as f:
        return [Tuyen(t) for t in json.load(f)["tuyen"]]


def them_nhieu(p, m):
    """Dịch điểm ngẫu nhiên, độ lệch chuẩn m/2 mét mỗi trục (≈ 95 % điểm trong vòng ± m)."""
    if m <= 0:
        return p
    dy, dx = random.gauss(0, m / 2), random.gauss(0, m / 2)
    return p[0] + dy / 111320, p[1] + dx / (111320 * math.cos(math.radians(p[0])))


async def main():
    ap = argparse.ArgumentParser(description="Tài xế giả lập gửi vị trí qua MQTT")
    ap.add_argument("ten")
    ap.add_argument("ma_xe")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=1883)
    ap.add_argument("--tuyen", type=int, default=0)
    ap.add_argument("--toc-do", type=float, default=30)
    ap.add_argument("--chu-ky", type=float, default=1)
    ap.add_argument("--nhieu", type=float, default=5)
    ap.add_argument("--bat-dau", type=float, default=0.0, help="bắt đầu ở phần (0–1) của tuyến")
    ap.add_argument("--thoi-gian", type=float, default=0)
    ap.add_argument("--xe", default="")
    ap.add_argument("--mat-song", action="store_true")
    a = ap.parse_args()
    t0 = time.time()

    def log(text):
        print(f"[{a.ten}] {time.time() - t0:5.1f}s {text}", flush=True)

    tuyen = doc_tuyen()[a.tuyen]
    chu_de, trang_thai = f"xe/{a.ma_xe}/vitri", f"xe/{a.ma_xe}/trangthai"
    k = MqttKhach(f"taixe-{a.ma_xe}", keepalive=10)
    await k.ket_noi(a.host, a.cong, di_chuc=(trang_thai, b"mat_ket_noi", True))
    k.phat(trang_thai, b"truc_tuyen", retain=True)
    log(f"MQTT {a.host}:{a.cong} – Last Will {trang_thai}=mat_ket_noi; tuyến {tuyen.ten} ({tuyen.dai / 1000:.1f} km), "
        f"{a.toc_do:.0f} km/h, gửi mỗi {a.chu_ky:g} s, nhiễu ± {a.nhieu:g} m")
    s, n = tuyen.dai * a.bat_dau, 0
    while True:
        (lat, lon), h = tuyen.tai(s)
        lat, lon = them_nhieu((lat, lon), a.nhieu)
        con_lai = None if tuyen.vong else max(0.0, tuyen.dai - s)
        den_noi = con_lai == 0
        tin = {"lat": round(lat, 6), "lon": round(lon, 6), "toc_do": 0 if den_noi else a.toc_do, "huong": round(h),
               "do_chinh_xac": a.nhieu, "nguon": "gia_lap", "t": int(time.time() * 1000), "ten": a.ten, "xe": a.xe,
               "tuyen": tuyen.ten, "dich": {"lat": tuyen.diem[-1][0], "lon": tuyen.diem[-1][1], "ten": tuyen.diem_den}}
        if con_lai is not None:
            tin["con_lai_m"] = round(con_lai)
        k.phat(chu_de, json.dumps(tin, ensure_ascii=False).encode(), retain=True)
        n += 1
        if n == 1 or n % 5 == 0 or den_noi:
            log(f"#{n} {lat:.5f},{lon:.5f} hướng {h:.0f}°" + (f", còn {con_lai / 1000:.2f} km" if con_lai is not None else ""))
        if den_noi:
            log("đã tới điểm đón")
            break
        if a.thoi_gian and time.time() - t0 + a.chu_ky > a.thoi_gian:
            break
        await asyncio.sleep(a.chu_ky)
        s += a.toc_do / 3.6 * a.chu_ky
    if a.mat_song:
        log(f"mất sóng: đóng TCP không gửi DISCONNECT (đã gửi {n} điểm)")
        await k.dong(dep=False)
    else:
        k.phat(trang_thai, b"nghi", retain=True)
        log(f"nghỉ: gửi {trang_thai}=nghi rồi DISCONNECT (đã gửi {n} điểm)")
        await k.dong()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
