"""Bài mở rộng MR6.15 – Khách theo dõi xe: SUBSCRIBE vị trí tài xế qua MQTT (giống màn hình "Tài xế đang tới").

Đăng ký xe/<mã xe>/vitri và xe/<mã xe>/trangthai. Mỗi điểm nhận: khoảng cách còn lại (con_lai_m của tài xế nếu có,
không thì đường chim bay × 1,3 tới điểm đón) và thời gian đến ≈ khoảng cách / tốc độ (tối thiểu 15 km/h trong phố).
Vì vị trí được gửi kèm cờ retain, khách vào sau vẫn thấy ngay điểm cuối cùng; trạng thái "mat_ket_noi" là Last Will
broker phát thay tài xế khi tài xế mất sóng.

Chạy:  python khach_theo_doi.py <tên> <mã xe> [host] [cổng_mqtt] [--don LAT LON] [--thoi-gian 0] [--moi 5]
"""
import argparse
import asyncio
import json
import time

from mqtt_khach import MqttKhach
from tai_xe_gia import khoang_cach


async def main():
    ap = argparse.ArgumentParser(description="Khách theo dõi vị trí tài xế qua MQTT")
    ap.add_argument("ten")
    ap.add_argument("ma_xe")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=1883)
    ap.add_argument("--don", type=float, nargs=2, help="điểm đón (lat lon); không có thì dùng 'dich' tài xế gửi")
    ap.add_argument("--thoi-gian", type=float, default=0)
    ap.add_argument("--moi", type=int, default=5, help="in một dòng mỗi N điểm")
    a = ap.parse_args()
    t0 = time.time()
    dem = {"n": 0}

    def log(text):
        print(f"[{a.ten}] {time.time() - t0:5.1f}s {text}", flush=True)

    def khi_nhan(chu_de, nd, giu_lai):
        if chu_de.endswith("/trangthai"):
            log(f"trạng thái tài xế: {nd.decode()}{' (tin giữ lại)' if giu_lai else ''}")
            return
        v = json.loads(nd)
        dem["n"] += 1
        don = tuple(a.don) if a.don else ((v["dich"]["lat"], v["dich"]["lon"]) if v.get("dich") else None)
        if "con_lai_m" in v:
            con = v["con_lai_m"]
        elif don:
            con = khoang_cach((v["lat"], v["lon"]), don) * 1.3          # hệ số đường phố so với chim bay
        else:
            con = None
        cu = (time.time() * 1000 - v["t"]) / 1000
        if dem["n"] == 1 or dem["n"] % a.moi == 0 or giu_lai or con == 0:
            if con is None:
                eta = "chưa biết điểm đón"
            elif con < 30:
                eta = "tài xế đã tới"
            else:
                phut = con / 1000 / max(v.get("toc_do", 0), 15) * 60
                eta = f"còn {con / 1000:.2f} km, khoảng {max(1, round(phut))} phút"
            nguon = "GPS thật" if v.get("nguon") == "gps" else "giả lập"
            log(f"{v['ten']} ({nguon}) {v['lat']:.5f},{v['lon']:.5f} {v.get('toc_do', 0):.0f} km/h – {eta}"
                f"{' – điểm giữ lại khi mới đăng ký' if giu_lai else ''} (trễ {cu:.2f} s)")

    k = MqttKhach(f"khach-{a.ten}-{int(t0)}", khi_nhan)
    await k.ket_noi(a.host, a.cong)
    k.dang_ky(f"xe/{a.ma_xe}/vitri", f"xe/{a.ma_xe}/trangthai")
    log(f"đăng ký xe/{a.ma_xe}/vitri, xe/{a.ma_xe}/trangthai")
    try:
        if a.thoi_gian:
            await asyncio.sleep(a.thoi_gian)
        else:
            await asyncio.Event().wait()
    finally:
        log(f"nhận {dem['n']} điểm vị trí")
        await k.dong()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
