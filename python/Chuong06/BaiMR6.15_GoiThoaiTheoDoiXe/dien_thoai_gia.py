"""Bài mở rộng MR6.15 – "Điện thoại giả": client dòng lệnh nói đúng giao thức của app Android (WebSocket + WebRTC).

Dùng để kiểm thử máy chủ (bản Python hoặc Go) khi chưa có điện thoại thật, và để chạy kịch bản demo tự động.
Thay cho micro, máy phát một âm sin tần số riêng (--tan-so, ví dụ An 440 Hz, Hùng 660 Hz); âm nhận được qua máy chủ
được phân tích FFT, nên in ra được "nghe thấy 660 Hz" – bằng chứng âm thanh đã đi A -> máy chủ -> B. Mỗi 2 s gửi
thống kê RTT / mất gói (lấy từ RTCP qua getStats) cho máy chủ, như app.

Chạy:  python dien_thoai_gia.py <tên> <mã> [host] [cổng] [--vai-tro khach|tai_xe] [--goi MÃ GIÂY]…
           [--tra-loi tu_dong|tu_choi|khong] [--ly-do "Tôi đang tới"] [--noi 8] [--giu-may 3 5] [--tan-so 440]
           [--ghi-am] [--lich-su] [--thoi-gian 20] [--tls --van-tay AA:BB:…]
"""
import argparse
import asyncio
import fractions
import hashlib
import json
import math
import ssl
import sys
import time

import aiohttp
import av
import numpy as np
from aiortc import MediaStreamTrack, RTCConfiguration, RTCPeerConnection, RTCSessionDescription

# Windows: khi chuyển hướng đầu ra vào tệp, Python dùng bảng mã ANSI (cp1252) và báo lỗi với chữ Việt -> ép UTF-8.
for _luong in (sys.stdout, sys.stderr):
    if hasattr(_luong, "reconfigure"):
        _luong.reconfigure(encoding="utf-8", errors="replace")

MAU, KHUNG = 48000, 960


class AmSin(MediaStreamTrack):
    """Micro giả: âm sin, khung 20 ms, tự giữ nhịp. tat=True: im lặng (tắt micro)."""
    kind = "audio"

    def __init__(self, tan_so):
        super().__init__()
        self.tan_so, self.tat = tan_so, False
        self._t0, self._pts = None, 0

    async def recv(self):
        if self._t0 is None:
            self._t0 = time.time()
        else:
            self._pts += KHUNG
            cho = self._t0 + self._pts / MAU - time.time()
            if cho > 0:
                await asyncio.sleep(cho)
        n = np.arange(self._pts, self._pts + KHUNG)
        x = np.zeros(KHUNG) if self.tat else 0.3 * np.sin(2 * math.pi * self.tan_so * n / MAU)
        f = av.AudioFrame.from_ndarray((x * 32767).astype(np.int16).reshape(1, -1), format="s16", layout="mono")
        f.sample_rate, f.pts, f.time_base = MAU, self._pts, fractions.Fraction(1, MAU)
        return f


class DienThoai:
    def __init__(self, a):
        self.a = a
        self.ten = a.ten
        self.ws = None
        self.ma_cuoc = None
        self.pc = None
        self.mic = AmSin(a.tan_so)
        self.mau_nhan = []                    # (thời điểm, mẫu mono float) của âm nhận được
        self.ten_kia = "?"
        self.t0 = time.time()
        self.thong_ke_task = None

    def log(self, text):
        print(f"[{self.ten}] {time.time() - self.t0:5.1f}s {text}", flush=True)

    async def gui(self, **tin):
        await self.ws.send_str(json.dumps(tin, ensure_ascii=False))

    # ---------------------------------------------------------------- WebRTC
    async def bat_dau_am_thanh(self):
        self.pc = pc = RTCPeerConnection(RTCConfiguration(iceServers=[]))
        pc.addTrack(self.mic)

        @pc.on("track")
        def khi_co_track(track):
            async def doc():
                try:
                    while True:
                        f = await track.recv()
                        x = f.to_ndarray().astype(np.float32).reshape(-1)
                        if f.layout.name == "stereo":
                            x = x[::2]
                        self.mau_nhan.append((time.time(), x / 32768.0))
                except Exception:
                    pass
            asyncio.ensure_future(doc())

        @pc.on("connectionstatechange")
        async def doi():
            self.log(f"WebRTC tới máy chủ: {pc.connectionState}")

        await pc.setLocalDescription(await pc.createOffer())
        await self.gui(loai="sdp", ma_cuoc=self.ma_cuoc, sdp={"type": "offer", "sdp": pc.localDescription.sdp})
        self.thong_ke_task = asyncio.ensure_future(self.gui_thong_ke())

    async def gui_thong_ke(self):
        while True:
            await asyncio.sleep(2)
            try:
                st = await self.pc.getStats()
            except Exception:
                return
            rtt = [s.roundTripTime for s in st.values() if s.type == "remote-inbound-rtp" and s.roundTripTime]
            mat = [s.fractionLost for s in st.values() if s.type == "remote-inbound-rtp"]
            tk = {"rtt_ms": round(rtt[0] * 1000, 1) if rtt else None,
                  "mat_goi": round(mat[0] * 100, 2) if mat else None}
            if self.ma_cuoc:
                await self.gui(loai="thong_ke", ma_cuoc=self.ma_cuoc, **tk)

    def phan_tich(self, tu=None, den=None):
        """Tần số trội và mức âm (dBFS) của phần âm nhận trong khoảng thời gian [tu, den)."""
        x = [m for t, m in self.mau_nhan if (tu is None or t >= tu) and (den is None or t < den)]
        if not x:
            return None, -120.0
        x = np.concatenate(x)
        muc = 20 * math.log10(max(float(np.sqrt(np.mean(x * x))), 1e-6))
        if muc < -50:
            return None, muc
        ph = np.abs(np.fft.rfft(x * np.hanning(len(x))))
        return float(np.argmax(ph) * MAU / len(x)), muc

    async def ket_thuc_am_thanh(self):
        if self.thong_ke_task:
            self.thong_ke_task.cancel()
        if self.pc:
            await self.pc.close()
            self.pc = None
        f, muc = self.phan_tich()
        if f is not None:
            self.log(f"đã nghe thấy âm {f:.0f} Hz của {self.ten_kia} (mức {muc:.1f} dBFS)")
        elif self.mau_nhan:
            self.log("chỉ nhận được im lặng")

    # ---------------------------------------------------------------- kịch bản
    async def hen(self, giay, viec):
        await asyncio.sleep(max(0.0, self.t0 + giay - time.time()))
        await viec()

    async def chay(self):
        a = self.a
        ngu_canh = None
        if a.tls:
            ngu_canh = ssl.create_default_context()
            ngu_canh.check_hostname, ngu_canh.verify_mode = False, ssl.CERT_NONE    # tự ký: kiểm bằng vân tay dưới
        url = f"{'wss' if a.tls else 'ws'}://{a.host}:{a.cong}/ws"
        async with aiohttp.ClientSession() as phien:
            async with phien.ws_connect(url, ssl=ngu_canh) as ws:
                if a.tls:
                    der = ws.get_extra_info("ssl_object").getpeercert(binary_form=True)
                    vt = hashlib.sha256(der).hexdigest().upper()
                    vt = ":".join(vt[i:i + 2] for i in range(0, len(vt), 2))
                    if a.van_tay and a.van_tay.upper() != vt:
                        self.log("SAI vân tay chứng chỉ máy chủ – ngắt (có thể bị giả mạo)")
                        return
                    self.log(f"TLS, vân tay máy chủ {vt[:23]}…")
                self.ws = ws
                await self.gui(loai="dang_nhap", ma=a.ma, ten=a.ten, vai_tro=a.vai_tro, bien_so=a.bien_so, xe=a.xe,
                               chuyen=a.chuyen, diem_don=a.diem_don, dong_y_ghi_am=a.ghi_am)
                viec = []
                for ma_den, luc in a.goi or []:
                    async def goi(ma_den=ma_den):
                        self.log(f"> gọi {ma_den}")
                        await self.gui(loai="goi", den=ma_den)
                    viec.append(asyncio.ensure_future(self.hen(float(luc), goi)))
                if a.thoi_gian:
                    viec.append(asyncio.ensure_future(self.hen(a.thoi_gian, ws.close)))
                try:
                    async for msg in ws:
                        if msg.type == aiohttp.WSMsgType.TEXT:
                            await self.nhan(json.loads(msg.data))
                finally:
                    for v in viec:
                        v.cancel()
                    if self.pc:
                        await self.ket_thuc_am_thanh()

    async def nhan(self, t):
        a, loai = self.a, t.get("loai")
        if loai == "dang_nhap_ok":
            self.log(f"đăng nhập máy chủ {t.get('may_chu')} (mã xe {t.get('ma_xe')})")
        elif loai == "danh_ba":
            on = ", ".join(x["ten"] for x in t["ds"] if x["truc_tuyen"] and x["ma"] != a.ma) or "–"
            if on != getattr(self, "_danh_ba", None):             # chỉ in khi có người vào / rời mạng
                self._danh_ba = on
                self.log("danh bạ trực tuyến: " + on)
        elif loai == "do_chuong":
            self.ma_cuoc, self.ten_kia = t["ma_cuoc"], t["den"]["ten"]
            self.log(f"đang đổ chuông bên {self.ten_kia} ({t['den']['mo_ta']})")
        elif loai == "cuoc_goi_den":
            self.ma_cuoc, self.ten_kia = t["ma_cuoc"], t["tu"]["ten"]
            self.log(f"CUỘC GỌI ĐẾN từ {self.ten_kia} ({t['tu']['mo_ta']}){' – sẽ được ghi âm' if t.get('ghi_am') else ''}")
            if a.tra_loi == "tu_dong":
                async def tl():
                    self.log("> trả lời")
                    await self.gui(loai="tra_loi", ma_cuoc=t["ma_cuoc"])
                asyncio.ensure_future(self.hen(time.time() - self.t0 + 1.5, tl))
            elif a.tra_loi == "tu_choi":
                async def tc():
                    self.log(f"> từ chối: {a.ly_do}")
                    await self.gui(loai="tu_choi", ma_cuoc=t["ma_cuoc"], ly_do=a.ly_do)
                asyncio.ensure_future(self.hen(time.time() - self.t0 + 1.0, tc))
        elif loai == "da_tra_loi":
            self.log("đã kết nối cuộc gọi" + (" – đang ghi âm" if t.get("ghi_am") else ""))
            await self.bat_dau_am_thanh()
            dau = time.time() - self.t0
            if a.giu_may:
                async def giu(bat):
                    self.log(f"> {'giữ máy' if bat else 'bỏ giữ máy'}")
                    await self.gui(loai="giu_may", ma_cuoc=self.ma_cuoc, bat=bat)
                    if not bat:
                        f, muc = self.phan_tich(self.t0 + dau + a.giu_may[0] + 0.5, self.t0 + dau + a.giu_may[1])
                        self.log(f"trong lúc giữ máy nghe được mức {muc:.0f} dBFS ({'im lặng' if f is None else f'{f:.0f} Hz'})")
                asyncio.ensure_future(self.hen(dau + a.giu_may[0], lambda: giu(True)))
                asyncio.ensure_future(self.hen(dau + a.giu_may[1], lambda: giu(False)))
            if a.noi:
                async def cup():
                    self.log("> kết thúc cuộc gọi")
                    await self.gui(loai="ket_thuc", ma_cuoc=self.ma_cuoc)
                asyncio.ensure_future(self.hen(dau + a.noi, cup))
        elif loai == "sdp":
            await self.pc.setRemoteDescription(RTCSessionDescription(**t["sdp"]))
        elif loai == "doi_phuong":
            if "giu_may" in t:
                self.log(f"{self.ten_kia} {'đang giữ máy' if t['giu_may'] else 'đã quay lại'}")
            if "tat_mic" in t:
                self.log(f"{self.ten_kia} {'tắt' if t['tat_mic'] else 'bật'} micro")
        elif loai == "da_ket_thuc":
            self.log(f"cuộc gọi kết thúc: {t['ly_do']} ({t['ket_qua']}, {t['thoi_luong']:.1f} s)")
            self.ma_cuoc = None                       # trước khi đóng: vòng thống kê không gửi thêm tin cho cuộc cũ
            await self.ket_thuc_am_thanh()
            if a.lich_su:
                await self.gui(loai="lich_su")
        elif loai == "lich_su":
            for x in t["ds"][:3]:
                kia = x["nghe"] if x["goi"]["ma"] == a.ma else x["goi"]
                bg = ", ".join(f"{k}: {v}" for k, v in x["ban_ghi"].items()) or "không ghi âm"
                self.log(f"lịch sử: {x['bat_dau']} {kia['ten']} {x['ket_qua']} {x['thoi_luong']:.0f} s – {bg}")
        elif loai == "loi":
            self.log(f"LỖI: {t.get('ly_do')}")


def main():
    ap = argparse.ArgumentParser(description="Điện thoại giả (kiểm thử MR6.15)")
    ap.add_argument("ten")
    ap.add_argument("ma")
    ap.add_argument("host", nargs="?", default="127.0.0.1")
    ap.add_argument("cong", nargs="?", type=int, default=8080)
    ap.add_argument("--vai-tro", default="khach", choices=["khach", "tai_xe"])
    ap.add_argument("--bien-so", default="")
    ap.add_argument("--xe", default="")
    ap.add_argument("--chuyen", default="")
    ap.add_argument("--diem-don", default="")
    ap.add_argument("--goi", nargs=2, action="append", metavar=("MA", "LUC"), help="gọi MA ở giây LUC (lặp được)")
    ap.add_argument("--tra-loi", default="tu_dong", choices=["tu_dong", "tu_choi", "khong"])
    ap.add_argument("--ly-do", default="Tôi đang tới")
    ap.add_argument("--noi", type=float, default=0)
    ap.add_argument("--giu-may", type=float, nargs=2)
    ap.add_argument("--tan-so", type=float, default=440)
    ap.add_argument("--ghi-am", action="store_true")
    ap.add_argument("--lich-su", action="store_true")
    ap.add_argument("--thoi-gian", type=float, default=0)
    ap.add_argument("--tls", action="store_true")
    ap.add_argument("--van-tay", default="")
    try:
        asyncio.run(DienThoai(ap.parse_args()).chay())
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
