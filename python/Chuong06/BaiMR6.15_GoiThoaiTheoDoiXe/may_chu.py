"""Bài mở rộng MR6.15 – Máy chủ trung gian Gọi thoại & Theo dõi tài xế (bản Python: aiohttp + aiortc).

Một tiến trình phục vụ ba việc, như "tổng đài ẩn số" của các ứng dụng gọi xe:
  1. Báo hiệu cuộc gọi qua WebSocket (JSON, đường /ws): đăng nhập, danh bạ ai đang trực tuyến, gọi – đổ chuông –
     trả lời/từ chối/hủy – kết thúc, trao đổi SDP và ứng viên ICE, giữ máy, tắt micro, thống kê RTT/mất gói, lịch sử.
  2. Đường âm thanh WebRTC qua máy chủ (kiểu SFU): mỗi điện thoại chỉ có MỘT kết nối WebRTC tới máy chủ, máy chủ
     chuyển tiếp âm thanh của bên này sang bên kia, nên hai máy không bao giờ biết địa chỉ IP / số điện thoại của nhau.
     Máy chủ ghi âm từng bên ra tệp .ogg (Opus) và sau cuộc gọi trộn hai bên thành một tệp bằng ffmpeg (nếu có).
     Lưu ý: aiortc giải mã gói Opus nhận được thành mẫu PCM rồi mã hóa lại khi gửi đi; bản Go (Pion) chỉ chuyển
     tiếp gói RTP, không giải mã – nhẹ hơn nhiều (so sánh ở README).
  3. MQTT broker (mqtt_broker.py, cổng 1883) cho vị trí tài xế: chủ đề xe/<mã xe>/vitri (1 điểm/giây, QoS 0, retain)
     và xe/<mã xe>/trangthai (Last Will "mat_ket_noi"). Máy chủ tự đăng ký xe/# để vẽ tài xế trên bảng điều khiển.
Trang bảng điều khiển: http://<máy chủ>:<cổng>/ (bang_dieu_khien.html, lấy số liệu từ /api/trangthai mỗi giây).

Giao thức báo hiệu (mỗi tin là một đối tượng JSON; cùng giao thức với bản Go và app Android):
  app -> máy chủ: {"loai":"dang_nhap","ma","ten","vai_tro":"khach"|"tai_xe","bien_so","xe","chuyen","diem_don",
                   "dong_y_ghi_am"}  {"loai":"goi","den":ma}  {"loai":"tra_loi"|"huy"|"ket_thuc","ma_cuoc"}
                  {"loai":"tu_choi","ma_cuoc","ly_do"}  {"loai":"sdp","ma_cuoc","sdp":{"type":"offer","sdp"}}
                  {"loai":"ice","ma_cuoc","ung_vien":{"candidate","sdpMid","sdpMLineIndex"}}
                  {"loai":"giu_may"|"tat_mic","ma_cuoc","bat":bool}  {"loai":"thong_ke","ma_cuoc","rtt_ms","mat_goi"}
                  {"loai":"lich_su"}
  máy chủ -> app: {"loai":"dang_nhap_ok","ma"}  {"loai":"danh_ba","ds":[...]}  {"loai":"do_chuong","ma_cuoc","den"}
                  {"loai":"cuoc_goi_den","ma_cuoc","tu","ghi_am"}  {"loai":"da_tra_loi","ma_cuoc","ghi_am"}
                  {"loai":"da_ket_thuc","ma_cuoc","ket_qua","ly_do","thoi_luong"}  {"loai":"sdp","ma_cuoc","sdp"}
                  {"loai":"doi_phuong","ma_cuoc","giu_may"|"tat_mic":bool}  {"loai":"lich_su","ds":[...]}
                  {"loai":"loi","ly_do"}
Luồng một cuộc gọi: A gửi goi -> máy chủ gửi do_chuong cho A, cuoc_goi_den cho B (đổ chuông tối đa 30 s) ->
  B gửi tra_loi -> máy chủ gửi da_tra_loi cho cả hai -> MỖI bên tạo RTCPeerConnection, gửi offer (sdp) -> máy chủ
  trả answer, nhận ICE -> âm thanh A -> máy chủ -> B và ngược lại -> một bên gửi ket_thuc -> da_ket_thuc cho cả hai.

Chạy:  python may_chu.py [host] [cổng_http] [cổng_mqtt] [--tls] [--thu-muc ban_ghi]
       (mặc định 0.0.0.0 8080 1883; --tls: wss/https trên cổng_http với chứng chỉ tự ký, in vân tay SHA-256)
Chỉ dùng trong mạng lab; ghi âm chỉ khi người gọi đồng ý (người nghe trả lời là đồng ý, như thông báo trên app).
"""
import argparse
import asyncio
import fractions
import json
import os
import re
import shutil
import ssl
import sys
import time
from collections import deque

import av
from aiohttp import WSMsgType, web
from aiortc import MediaStreamTrack, RTCConfiguration, RTCPeerConnection, RTCSessionDescription
from aiortc.contrib.media import MediaRelay
from aiortc.sdp import candidate_from_sdp

from mqtt_broker import MqttBroker

# Windows: khi chuyển hướng đầu ra vào tệp, Python dùng bảng mã ANSI (cp1252) và báo lỗi với chữ Việt -> ép UTF-8.
for _luong in (sys.stdout, sys.stderr):
    if hasattr(_luong, "reconfigure"):
        _luong.reconfigure(encoding="utf-8", errors="replace")

THU_MUC = os.path.dirname(os.path.abspath(__file__))
DO_CHUONG_TOI_DA = 30.0
MAU, KHUNG = 48000, 960                            # 48 kHz, khung 20 ms


def log(text):
    print(f"[may_chu] {time.strftime('%H:%M:%S')} {text}", flush=True)


def mm_ss(giay):
    giay = int(giay)
    return f"{giay // 60:02d}:{giay % 60:02d}"


# ===================================================================== âm thanh qua máy chủ
class TiepAm(MediaStreamTrack):
    """Track âm thanh máy chủ gửi tới MỘT điện thoại: lấy khung từ track của bên kia (nếu có, và không giữ máy),
    thiếu khung thì phát im lặng; tự giữ nhịp 20 ms. Hàng đợi tối đa 5 khung (100 ms) – đầy thì bỏ khung cũ để
    độ trễ không tăng dần khi đồng hồ hai bên lệch nhau."""
    kind = "audio"

    def __init__(self):
        super().__init__()
        self.hang = deque(maxlen=5)
        self.tam_dung = False
        self._nguon_task = None
        self._t0, self._pts = None, 0

    def dat_nguon(self, track):
        async def keo():
            try:
                while True:
                    self.hang.append(await track.recv())
            except Exception:                        # nguồn kết thúc (bên kia cúp máy / mất kết nối)
                pass
        if self._nguon_task:
            self._nguon_task.cancel()
        self._nguon_task = asyncio.ensure_future(keo())

    async def recv(self):
        if self._t0 is None:
            self._t0 = time.time()
        else:
            self._pts += KHUNG
            cho = self._t0 + self._pts / MAU - time.time()
            if cho > 0:
                await asyncio.sleep(cho)
        f = self.hang.popleft() if self.hang else None
        if f is None or self.tam_dung:
            f = av.AudioFrame(format="s16", layout="stereo", samples=KHUNG)   # cùng dạng khung bộ giải mã Opus trả về
            for p in f.planes:
                p.update(bytes(p.buffer_size))
            f.sample_rate = MAU
        f.pts, f.time_base = self._pts, fractions.Fraction(1, MAU)
        return f

    def stop(self):
        if self._nguon_task:
            self._nguon_task.cancel()
        super().stop()


class GhiAm:
    """Ghi một track âm thanh ra tệp Ogg/Opus mono 24 kbit/s (PyAV/libopus). t0 = thời điểm khung đầu tiên."""

    def __init__(self, track, duong_dan):
        self.duong_dan, self.t0, self.so_khung = duong_dan, None, 0
        self.task = asyncio.ensure_future(self._chay(track))

    async def _chay(self, track):
        c = av.open(self.duong_dan, "w", format="ogg")
        s = c.add_stream("libopus", rate=MAU, layout="mono")
        s.bit_rate = 24000
        doi = av.AudioResampler(format="s16", layout="mono", rate=MAU)
        pts = 0
        try:
            while True:
                f = await track.recv()
                if self.t0 is None:
                    self.t0 = time.time()
                self.so_khung += 1
                for g in doi.resample(f):
                    g.pts, g.time_base = pts, fractions.Fraction(1, MAU)
                    pts += g.samples
                    for p in s.encode(g):
                        c.mux(p)
        except (Exception, asyncio.CancelledError):
            pass
        finally:
            try:
                for p in s.encode(None):
                    c.mux(p)
            except Exception:
                pass
            c.close()

    async def dong(self):
        self.task.cancel()
        try:
            await self.task
        except (Exception, asyncio.CancelledError):
            pass


class DuongAm:
    """Phần của MỘT bên trong cuộc gọi: kết nối WebRTC tới máy chủ, track gửi đi, bộ ghi âm."""

    def __init__(self, nd):
        self.nd = nd
        self.pc = None
        self.tiep = TiepAm()
        self.nguon = None                   # track nhận từ điện thoại của bên này
        self.ghi = None
        self.ung_vien_cho = []              # ICE tới trước khi có pc
        self.rtt_ms = self.mat_goi = None


# ===================================================================== người dùng, cuộc gọi
class NguoiDung:
    def __init__(self, ma):
        self.ma, self.ten, self.vai_tro = ma, ma, "khach"
        self.bien_so = self.xe = self.chuyen = self.diem_don = ""
        self.dong_y_ghi_am = False
        self.ws = None
        self.cuoc = None

    def ma_xe(self):
        return re.sub(r"[^0-9A-Za-z]", "", self.bien_so).upper() or self.ma

    def mo_ta(self):
        if self.vai_tro == "tai_xe":
            p = ["Tài xế"] + [x for x in (self.bien_so, self.xe) if x]
        else:
            p = ["Khách"] + ([f"chuyến {self.chuyen}"] if self.chuyen else []) + \
                ([f"đón tại {self.diem_don}"] if self.diem_don else [])
        return " · ".join(p)

    def the(self):
        return {"ma": self.ma, "ten": self.ten, "vai_tro": self.vai_tro, "mo_ta": self.mo_ta(), "ma_xe": self.ma_xe(),
                "truc_tuyen": self.ws is not None, "ban": self.cuoc is not None}

    async def gui(self, tin):
        if self.ws is not None and not self.ws.closed:
            try:
                await self.ws.send_str(json.dumps(tin, ensure_ascii=False))
            except ConnectionError:
                pass


class CuocGoi:
    def __init__(self, ma, goi, nghe):
        self.ma, self.goi, self.nghe = ma, goi, nghe
        self.trang_thai = "do_chuong"
        self.t_tao, self.t_tra_loi, self.t_ket_thuc = time.time(), None, None
        self.ghi_am = goi.dong_y_ghi_am
        self.giu_may = False
        self.duong = {}                       # mã người -> DuongAm
        self.ket_qua = self.ly_do = None
        self.han_chuong = None

    def ben_kia(self, nd):
        return self.nghe if nd is self.goi else self.goi

    def thoi_luong(self):
        if not self.t_tra_loi:
            return 0
        return (self.t_ket_thuc or time.time()) - self.t_tra_loi


class MayChu:
    def __init__(self, thu_muc_ghi, ban="Python (aiortc)"):
        self.ban = ban
        self.nguoi = {}                       # mã -> NguoiDung (cả người đã ngắt, để danh bạ có "Tất cả")
        self.cuoc = {}                        # mã cuộc đang đổ chuông / đang nói
        self.relay = MediaRelay()
        self.thu_muc_ghi = thu_muc_ghi
        os.makedirs(thu_muc_ghi, exist_ok=True)
        self.tep_lich_su = os.path.join(thu_muc_ghi, "lich_su.json")
        try:
            with open(self.tep_lich_su, encoding="utf-8") as f:
                self.lich_su = json.load(f)
        except (OSError, ValueError):
            self.lich_su = []
        self.t_bat_dau = time.time()
        self.ffmpeg = shutil.which("ffmpeg")
        self.mqtt = MqttBroker(lambda t: log(f"mqtt: {t}"))
        self.tai_xe = {}                      # mã xe -> vị trí mới nhất (từ MQTT)
        self.diem_vi_tri = deque()            # thời điểm nhận các điểm vị trí (tính điểm/giây)
        self.kbit_s = 0.0
        self.mqtt.dang_ky_noi_bo("xe/#", self._nhan_mqtt)
        self.dia_chi = ""

    # ---------------------------------------------------------------- MQTT: tài xế trên bản đồ
    def _nhan_mqtt(self, chu_de, nd):
        p = chu_de.split("/")
        if len(p) != 3:
            return
        x = self.tai_xe.setdefault(p[1], {"ma_xe": p[1], "vet": []})
        if p[2] == "vitri" and nd:
            try:
                v = json.loads(nd)
                x.update(lat=float(v["lat"]), lon=float(v["lon"]), toc_do=float(v.get("toc_do", 0)),
                         nguon=v.get("nguon", "gps"), ten=v.get("ten", p[1]), t_nhan=time.time(), trang_thai="truc_tuyen")
                x["vet"] = (x["vet"] + [[x["lat"], x["lon"]]])[-120:]
                self.diem_vi_tri.append(time.time())
            except (ValueError, KeyError, TypeError):
                pass
        elif p[2] == "trangthai":
            x["trang_thai"] = nd.decode("utf-8", "replace") or "?"

    # ---------------------------------------------------------------- danh bạ
    async def phat_danh_ba(self):
        ds = [n.the() for n in sorted(self.nguoi.values(), key=lambda n: (n.ws is None, n.ten))]
        for n in list(self.nguoi.values()):
            await n.gui({"loai": "danh_ba", "ds": ds})

    # ---------------------------------------------------------------- WebSocket báo hiệu
    async def xu_ly_ws(self, request):
        ws = web.WebSocketResponse(heartbeat=20)
        await ws.prepare(request)
        nd = None
        dia_chi = request.remote
        try:
            async for msg in ws:
                if msg.type != WSMsgType.TEXT:
                    continue
                try:
                    tin = json.loads(msg.data)
                    loai = tin.get("loai")
                except (ValueError, AttributeError):
                    await ws.send_str(json.dumps({"loai": "loi", "ly_do": "tin không phải JSON"}))
                    continue
                if loai == "dang_nhap":
                    nd = await self.dang_nhap(ws, tin, dia_chi)
                elif nd is None:
                    await ws.send_str(json.dumps({"loai": "loi", "ly_do": "chưa đăng nhập"}, ensure_ascii=False))
                else:
                    await self.xu_ly_tin(nd, loai, tin)
        finally:
            if nd is not None and nd.ws is ws:
                nd.ws = None
                log(f"{nd.ten} ({nd.ma}) rời mạng")
                if nd.cuoc:
                    await self.ket_thuc(self.cuoc.get(nd.cuoc), "mat_ket_noi", f"{nd.ten} mất kết nối")
                await self.phat_danh_ba()
        return ws

    async def dang_nhap(self, ws, tin, dia_chi):
        ma = str(tin.get("ma", "")).strip()
        if not ma or len(ma) > 20:
            await ws.send_str(json.dumps({"loai": "loi", "ly_do": "mã người dùng 1–20 ký tự"}, ensure_ascii=False))
            return None
        nd = self.nguoi.setdefault(ma, NguoiDung(ma))
        if nd.ws is not None and nd.ws is not ws:            # đăng nhập lại từ máy khác: đóng kết nối cũ
            await nd.ws.close()
        nd.ws = ws
        nd.ten = str(tin.get("ten") or ma)[:40]
        nd.vai_tro = "tai_xe" if tin.get("vai_tro") == "tai_xe" else "khach"
        for k in ("bien_so", "xe", "chuyen", "diem_don"):
            setattr(nd, k, str(tin.get(k) or "")[:40])
        nd.dong_y_ghi_am = bool(tin.get("dong_y_ghi_am"))
        log(f"{nd.ten} ({ma}, {nd.mo_ta()}) đăng nhập từ {dia_chi}")
        await nd.gui({"loai": "dang_nhap_ok", "ma": ma, "ma_xe": nd.ma_xe(), "may_chu": self.ban})
        await self.phat_danh_ba()
        return nd

    async def xu_ly_tin(self, nd, loai, tin):
        c = self.cuoc.get(tin.get("ma_cuoc"))
        if loai == "goi":
            await self.goi(nd, str(tin.get("den", "")))
        elif loai == "lich_su":
            ds = [x for x in self.lich_su if nd.ma in (x["goi"]["ma"], x["nghe"]["ma"])]
            await nd.gui({"loai": "lich_su", "ds": ds[-50:][::-1]})
        elif c is None or nd not in (c.goi, c.nghe):
            # Tin trễ của cuộc gọi vừa kết thúc (thống kê, ICE gửi trước khi nhận da_ket_thuc) là bình thường: bỏ qua.
            if loai not in ("thong_ke", "ice", "sdp", "giu_may", "tat_mic"):
                await nd.gui({"loai": "loi", "ly_do": "không có cuộc gọi này", "ma_cuoc": tin.get("ma_cuoc")})
        elif loai == "tra_loi" and nd is c.nghe and c.trang_thai == "do_chuong":
            c.trang_thai, c.t_tra_loi = "dang_noi", time.time()
            if c.han_chuong:
                c.han_chuong.cancel()
            log(f"{c.ma}: {nd.ten} trả lời")
            for x in (c.goi, c.nghe):
                await x.gui({"loai": "da_tra_loi", "ma_cuoc": c.ma, "ghi_am": c.ghi_am})
        elif loai == "tu_choi" and nd is c.nghe and c.trang_thai == "do_chuong":
            await self.ket_thuc(c, "tu_choi", str(tin.get("ly_do") or "Từ chối"))
        elif loai in ("huy", "ket_thuc"):
            await self.ket_thuc(c, "huy" if c.trang_thai == "do_chuong" else "hoan_thanh",
                                f"{nd.ten} {'hủy' if c.trang_thai == 'do_chuong' else 'kết thúc'}")
        elif loai == "sdp" and c.trang_thai == "dang_noi":
            await self.nhan_offer(c, nd, tin.get("sdp") or {})
        elif loai == "ice":
            await self.nhan_ice(c, nd, tin.get("ung_vien") or {})
        elif loai == "giu_may":
            c.giu_may = bool(tin.get("bat"))
            for d in c.duong.values():
                d.tiep.tam_dung = c.giu_may
            await c.ben_kia(nd).gui({"loai": "doi_phuong", "ma_cuoc": c.ma, "giu_may": c.giu_may})
        elif loai == "tat_mic":
            await c.ben_kia(nd).gui({"loai": "doi_phuong", "ma_cuoc": c.ma, "tat_mic": bool(tin.get("bat"))})
        elif loai == "thong_ke":
            d = c.duong.get(nd.ma)
            if d:
                d.rtt_ms, d.mat_goi = tin.get("rtt_ms"), tin.get("mat_goi")

    # ---------------------------------------------------------------- vòng đời cuộc gọi
    async def goi(self, nd, ma_den):
        den = self.nguoi.get(ma_den)
        if den is None or den.ws is None:
            await nd.gui({"loai": "loi", "ly_do": "người nhận không trực tuyến"})
            return
        if den is nd or nd.cuoc:
            await nd.gui({"loai": "loi", "ly_do": "không thể gọi lúc này"})
            return
        ma = time.strftime("call-%Y%m%d-%H%M%S")
        while ma in self.cuoc or any(x["ma_cuoc"] == ma for x in self.lich_su):
            ma += "x"
        c = CuocGoi(ma, nd, den)
        if den.cuoc:                                  # máy bận: kết thúc ngay, vẫn ghi lịch sử
            self.cuoc[ma] = c
            nd.cuoc = ma
            await nd.gui({"loai": "do_chuong", "ma_cuoc": ma, "den": den.the()})
            await self.ket_thuc(c, "ban", "Máy bận")
            return
        self.cuoc[ma] = c
        nd.cuoc = den.cuoc = ma
        log(f"{ma}: {nd.ten} gọi {den.ten}{' (ghi âm)' if c.ghi_am else ''}")
        await nd.gui({"loai": "do_chuong", "ma_cuoc": ma, "den": den.the()})
        await den.gui({"loai": "cuoc_goi_den", "ma_cuoc": ma, "tu": nd.the(), "ghi_am": c.ghi_am})
        await self.phat_danh_ba()

        async def het_gio():
            await asyncio.sleep(DO_CHUONG_TOI_DA)
            if c.trang_thai == "do_chuong":
                await self.ket_thuc(c, "nho", "Không trả lời")
        c.han_chuong = asyncio.ensure_future(het_gio())

    async def nhan_offer(self, c, nd, sdp):
        if sdp.get("type") != "offer" or not sdp.get("sdp"):
            await nd.gui({"loai": "loi", "ly_do": "sdp phải là offer"})
            return
        cu = c.duong.get(nd.ma)
        if cu and cu.pc:                              # gửi lại offer (kết nối lại): bỏ kết nối cũ
            await cu.pc.close()
        d = c.duong[nd.ma] = DuongAm(nd)
        if cu:
            d.ung_vien_cho = cu.ung_vien_cho
        pc = d.pc = RTCPeerConnection(RTCConfiguration(iceServers=[]))
        d.tiep.tam_dung = c.giu_may
        kia = c.duong.get(c.ben_kia(nd).ma)
        if kia and kia.nguon:                         # bên kia đã có tiếng: nối ngay
            d.tiep.dat_nguon(self.relay.subscribe(kia.nguon, buffered=False))

        @pc.on("track")
        def khi_co_track(track):
            if track.kind != "audio":
                return
            d.nguon = track
            log(f"{c.ma}: nhận âm thanh của {nd.ten}")
            kia2 = c.duong.get(c.ben_kia(nd).ma)
            if kia2:
                kia2.tiep.dat_nguon(self.relay.subscribe(track, buffered=False))
            if c.ghi_am and d.ghi is None:
                d.ghi = GhiAm(self.relay.subscribe(track, buffered=False),
                              os.path.join(self.thu_muc_ghi, f"{c.ma}-{nd.ma}.ogg"))

        @pc.on("connectionstatechange")
        async def khi_doi_trang_thai():
            log(f"{c.ma}: WebRTC của {nd.ten}: {pc.connectionState}")
            if pc.connectionState == "failed" and c.ma in self.cuoc:
                await self.ket_thuc(c, "mat_ket_noi", f"WebRTC của {nd.ten} thất bại")

        await pc.setRemoteDescription(RTCSessionDescription(sdp=sdp["sdp"], type="offer"))
        pc.addTrack(d.tiep)
        for u in d.ung_vien_cho:
            await self._them_ice(pc, u)
        d.ung_vien_cho = []
        await pc.setLocalDescription(await pc.createAnswer())     # aiortc chờ thu thập ICE xong: answer đủ ứng viên
        await nd.gui({"loai": "sdp", "ma_cuoc": c.ma,
                      "sdp": {"type": pc.localDescription.type, "sdp": pc.localDescription.sdp}})

    async def _them_ice(self, pc, u):
        cand = str(u.get("candidate") or "")
        if not cand:
            return
        try:
            ic = candidate_from_sdp(cand.split(":", 1)[1] if cand.startswith("candidate:") else cand)
            ic.sdpMid, ic.sdpMLineIndex = u.get("sdpMid"), u.get("sdpMLineIndex")
            await pc.addIceCandidate(ic)
        except (ValueError, IndexError):
            pass

    async def nhan_ice(self, c, nd, u):
        d = c.duong.get(nd.ma)
        if d is None:
            d = c.duong[nd.ma] = DuongAm(nd)
        if d.pc is None:
            d.ung_vien_cho.append(u)
        else:
            await self._them_ice(d.pc, u)

    async def ket_thuc(self, c, ket_qua, ly_do):
        if c is None or c.ma not in self.cuoc:
            return
        del self.cuoc[c.ma]
        c.trang_thai, c.ket_qua, c.ly_do, c.t_ket_thuc = "ket_thuc", ket_qua, ly_do, time.time()
        if c.han_chuong:
            c.han_chuong.cancel()
        for x in (c.goi, c.nghe):
            if x.cuoc == c.ma:
                x.cuoc = None
        log(f"{c.ma}: kết thúc ({ket_qua}: {ly_do}), thời lượng {mm_ss(c.thoi_luong())}")
        for x in (c.goi, c.nghe):
            await x.gui({"loai": "da_ket_thuc", "ma_cuoc": c.ma, "ket_qua": ket_qua, "ly_do": ly_do,
                         "thoi_luong": round(c.thoi_luong(), 1)})
        for d in c.duong.values():
            d.tiep.stop()
            if d.pc:
                await d.pc.close()
            if d.ghi:
                await d.ghi.dong()
        ban_ghi = {}
        for x in (c.goi, c.nghe):
            d = c.duong.get(x.ma)
            if d and d.ghi and d.ghi.so_khung and os.path.exists(d.ghi.duong_dan):
                ban_ghi[x.ma] = os.path.basename(d.ghi.duong_dan)
        rec = {"ma_cuoc": c.ma, "goi": {"ma": c.goi.ma, "ten": c.goi.ten}, "nghe": {"ma": c.nghe.ma, "ten": c.nghe.ten},
               "bat_dau": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(c.t_tao)),
               "thoi_luong": round(c.thoi_luong(), 1), "ket_qua": ket_qua, "ly_do": ly_do, "ban_ghi": ban_ghi}
        if len(ban_ghi) == 2:
            tron = await self.tron(c)
            if tron:
                ban_ghi["tron"] = tron
        if ban_ghi:
            tep = [ban_ghi["tron"]] if "tron" in ban_ghi else list(ban_ghi.values())
            rec["kich_thuoc"] = sum(os.path.getsize(os.path.join(self.thu_muc_ghi, f)) for f in tep)
        self.lich_su.append(rec)
        with open(self.tep_lich_su, "w", encoding="utf-8") as f:
            json.dump(self.lich_su[-500:], f, ensure_ascii=False, indent=1)
        await self.phat_danh_ba()

    async def tron(self, c):
        """Trộn hai bên thành <mã cuộc>.ogg bằng ffmpeg (adelay bù chênh thời điểm bắt đầu của hai tệp)."""
        if not self.ffmpeg:
            log("không có ffmpeg: bỏ qua bước trộn (app phát hai tệp cùng lúc)")
            return None
        a, b = c.duong[c.goi.ma].ghi, c.duong[c.nghe.ma].ghi
        tre = int(round((b.t0 - a.t0) * 1000))
        ra = os.path.join(self.thu_muc_ghi, f"{c.ma}.ogg")
        loc = (f"[1:a]adelay={tre}|{tre}[b];[0:a][b]" if tre >= 0 else f"[0:a]adelay={-tre}|{-tre}[a];[a][1:a]") + \
            "amix=inputs=2:duration=longest:normalize=0"
        p = await asyncio.create_subprocess_exec(self.ffmpeg, "-y", "-loglevel", "error", "-i", a.duong_dan, "-i",
                                                 b.duong_dan, "-filter_complex", loc, "-ac", "1", "-c:a", "libopus",
                                                 "-b:a", "32k", ra, stdout=asyncio.subprocess.DEVNULL,
                                                 stderr=asyncio.subprocess.PIPE)
        _, err = await p.communicate()
        if p.returncode != 0:
            log(f"ffmpeg lỗi: {err.decode(errors='replace').strip()[:200]}")
            return None
        log(f"{c.ma}: đã trộn hai bên -> {os.path.basename(ra)} (lệch {tre} ms)")
        return os.path.basename(ra)

    # ---------------------------------------------------------------- bảng điều khiển
    async def do_bang_thong(self):
        """Mỗi 2 s: băng thông máy chủ gửi đi (tổng bytesSent của các kết nối WebRTC)."""
        truoc = {}
        while True:
            await asyncio.sleep(2)
            tong, moi = 0, {}
            for c in list(self.cuoc.values()):
                for d in list(c.duong.values()):
                    if d.pc is None:
                        continue
                    try:
                        st = await d.pc.getStats()
                    except Exception:
                        continue
                    b = sum(getattr(s, "bytesSent", 0) for s in st.values() if s.type == "outbound-rtp")
                    moi[id(d.pc)] = b
                    tong += max(0, b - truoc.get(id(d.pc), b))
            truoc = moi
            self.kbit_s = tong * 8 / 2 / 1000

    def trang_thai(self):
        bay_gio = time.time()
        while self.diem_vi_tri and bay_gio - self.diem_vi_tri[0] > 5:
            self.diem_vi_tri.popleft()
        tt = [n for n in self.nguoi.values() if n.ws is not None]
        cuoc = []
        for c in self.cuoc.values():
            rtt = [d.rtt_ms for d in c.duong.values() if d.rtt_ms is not None]
            mat = [d.mat_goi for d in c.duong.values() if d.mat_goi is not None]
            cuoc.append({"ma": c.ma, "ai": f"{c.goi.ten} ↔ {c.nghe.ten}", "trang_thai": c.trang_thai,
                         "thoi_luong": mm_ss(c.thoi_luong()) if c.trang_thai == "dang_noi" else "đổ chuông",
                         "rtt_ms": max(rtt) if rtt else None, "mat_goi": max(mat) if mat else None,
                         "ghi_am": c.ghi_am, "giu_may": c.giu_may})
        ban_ghi = []
        for x in reversed(self.lich_su[-200:]):
            if x.get("ban_ghi"):
                ban_ghi.append({"tep": x["ban_ghi"].get("tron") or next(iter(x["ban_ghi"].values())),
                                "cac_tep": x["ban_ghi"], "ai": f"{x['goi']['ten']} ↔ {x['nghe']['ten']}",
                                "thoi_luong": mm_ss(x["thoi_luong"]), "kich_thuoc": x.get("kich_thuoc", 0),
                                "bat_dau": x["bat_dau"]})
            if len(ban_ghi) >= 8:
                break
        hom_nay = time.strftime("%Y-%m-%d")
        return {
            "may_chu": {"ban": self.ban, "dia_chi": self.dia_chi, "chay_s": int(bay_gio - self.t_bat_dau),
                        "ffmpeg": bool(self.ffmpeg)},
            "so_lieu": {"truc_tuyen": len(tt), "khach": sum(n.vai_tro == "khach" for n in tt),
                        "tai_xe": sum(n.vai_tro == "tai_xe" for n in tt),
                        "dang_goi": sum(c.trang_thai == "dang_noi" for c in self.cuoc.values()),
                        "cuoc_hom_nay": sum(x["bat_dau"].startswith(hom_nay) for x in self.lich_su),
                        "kbit_s": round(self.kbit_s, 1), "diem_s": round(len(self.diem_vi_tri) / 5, 1),
                        "mqtt_ket_noi": len(self.mqtt.ket_noi), "mqtt_tin": self.mqtt.so_tin},
            "cuoc": cuoc, "ban_ghi": ban_ghi,
            "tai_xe": [{"ma_xe": k, "ten": v.get("ten", k), "lat": v.get("lat"), "lon": v.get("lon"),
                        "toc_do": v.get("toc_do", 0), "nguon": v.get("nguon", "?"), "trang_thai": v.get("trang_thai", "?"),
                        "cach_s": round(bay_gio - v["t_nhan"], 1) if "t_nhan" in v else None, "vet": v["vet"][-60:]}
                       for k, v in sorted(self.tai_xe.items()) if "lat" in v],
        }


def tao_chung_chi(thu_muc):
    """Chứng chỉ tự ký cho https/wss trong lab (cryptography); trả (cert, key, vân tay SHA-256 của chứng chỉ)."""
    import datetime
    import hashlib
    from cryptography import x509
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import ec
    from cryptography.x509.oid import NameOID
    os.makedirs(thu_muc, exist_ok=True)
    cert, key = os.path.join(thu_muc, "may_chu.crt"), os.path.join(thu_muc, "may_chu.key")
    if not os.path.exists(cert):
        k = ec.generate_private_key(ec.SECP256R1())
        ten = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "may-chu-goi-thoai-lab")])
        bay_gio = datetime.datetime.now(datetime.timezone.utc)
        c = (x509.CertificateBuilder().subject_name(ten).issuer_name(ten).public_key(k.public_key())
             .serial_number(x509.random_serial_number()).not_valid_before(bay_gio - datetime.timedelta(days=1))
             .not_valid_after(bay_gio + datetime.timedelta(days=825)).sign(k, hashes.SHA256()))
        with open(key, "wb") as f:
            f.write(k.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8,
                                    serialization.NoEncryption()))
        with open(cert, "wb") as f:
            f.write(c.public_bytes(serialization.Encoding.PEM))
    with open(cert, "rb") as f:
        der = x509.load_pem_x509_certificate(f.read()).public_bytes(serialization.Encoding.DER)
    vt = hashlib.sha256(der).hexdigest().upper()
    return cert, key, ":".join(vt[i:i + 2] for i in range(0, len(vt), 2))


async def chay(host, cong, cong_mqtt, tls, thu_muc_ghi):
    mc = MayChu(thu_muc_ghi)
    app = web.Application()

    async def trang_chu(_):
        return web.FileResponse(os.path.join(THU_MUC, "bang_dieu_khien.html"))

    async def api_trang_thai(_):
        return web.json_response(mc.trang_thai(), dumps=lambda o: json.dumps(o, ensure_ascii=False))

    async def ban_ghi(request):
        ten = request.match_info["ten"]
        if not re.fullmatch(r"[A-Za-z0-9._-]+\.ogg", ten):
            raise web.HTTPNotFound()
        p = os.path.join(mc.thu_muc_ghi, ten)
        if not os.path.exists(p):
            raise web.HTTPNotFound()
        return web.FileResponse(p, headers={"Content-Type": "audio/ogg"})

    async def api_ngat(request):
        await mc.ket_thuc(mc.cuoc.get(request.match_info["ma"]), "hoan_thanh", "Máy chủ ngắt")
        return web.json_response({"ok": True})

    async def api_dung(request):
        if request.remote not in ("127.0.0.1", "::1"):           # chỉ cho dừng từ chính máy chủ
            raise web.HTTPForbidden(text="chỉ dừng được từ máy chủ (127.0.0.1)")
        asyncio.get_running_loop().call_later(0.3, dung.set)
        return web.json_response({"ok": True})

    app.router.add_get("/", trang_chu)
    app.router.add_get("/ws", mc.xu_ly_ws)
    app.router.add_get("/api/trangthai", api_trang_thai)
    app.router.add_post("/api/ngat/{ma}", api_ngat)
    app.router.add_post("/api/dung", api_dung)
    app.router.add_get("/banghi/{ten}", ban_ghi)
    ngu_canh = None
    if tls:
        cert, key, vt = tao_chung_chi(os.path.join(THU_MUC, "chung_chi"))
        ngu_canh = ssl.create_default_context(ssl.Purpose.CLIENT_AUTH)
        ngu_canh.load_cert_chain(cert, key)
        log(f"TLS: chứng chỉ tự ký, vân tay SHA-256 (nhập vào app): {vt}")
    runner = web.AppRunner(app, access_log=None)
    await runner.setup()
    await web.TCPSite(runner, host, cong, ssl_context=ngu_canh).start()
    await mc.mqtt.bat_dau(host, cong_mqtt)
    mc.dia_chi = f"{'wss' if tls else 'ws'}://{host}:{cong}/ws · MQTT {cong_mqtt}"
    log(f"Báo hiệu {mc.dia_chi}; bảng điều khiển {'https' if tls else 'http'}://{host}:{cong}/; "
        f"ghi âm vào {mc.thu_muc_ghi}{'' if mc.ffmpeg else ' (không có ffmpeg: không trộn hai bên)'}")
    dung = asyncio.Event()
    bt = asyncio.ensure_future(mc.do_bang_thong())
    try:
        await dung.wait()
    finally:
        bt.cancel()
        for c in list(mc.cuoc.values()):
            await mc.ket_thuc(c, "hoan_thanh", "Máy chủ dừng")
        log(f"Dừng. Đã phục vụ {len(mc.nguoi)} người dùng, {len(mc.lich_su)} cuộc gọi trong lịch sử, "
            f"{mc.mqtt.so_tin} tin MQTT")
        await runner.cleanup()


def main():
    ap = argparse.ArgumentParser(description="Máy chủ Gọi thoại & Theo dõi tài xế (MR6.15)")
    ap.add_argument("host", nargs="?", default="0.0.0.0")
    ap.add_argument("cong", nargs="?", type=int, default=8080)
    ap.add_argument("cong_mqtt", nargs="?", type=int, default=1883)
    ap.add_argument("--tls", action="store_true")
    ap.add_argument("--thu-muc", default=os.path.join(THU_MUC, "ban_ghi"))
    a = ap.parse_args()
    try:
        asyncio.run(chay(a.host, a.cong, a.cong_mqtt, a.tls, a.thu_muc))
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
