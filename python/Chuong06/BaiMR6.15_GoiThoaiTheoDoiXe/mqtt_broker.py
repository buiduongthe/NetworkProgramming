"""Bài mở rộng MR6.15 – MQTT broker tối giản (MQTT 3.1.1, TCP), viết tay bằng asyncio để thấy rõ giao thức.

Đủ cho bài theo dõi tài xế: tài xế PUBLISH vị trí lên chủ đề xe/<mã xe>/vitri (QoS 0, 1 điểm/giây, cờ retain để
khách vào sau nhận ngay điểm cuối), khách SUBSCRIBE đúng chủ đề chuyến của mình. Hỗ trợ:
  CONNECT/CONNACK (client id, keepalive, Last Will), PUBLISH QoS 0 và 1 (QoS 1 trả PUBACK, chuyển tiếp ở QoS 0),
  SUBSCRIBE/SUBACK (ký tự đại diện + và #), UNSUBSCRIBE/UNSUBACK, PINGREQ/PINGRESP, DISCONNECT, tin giữ lại
  (retained; payload rỗng = xóa), Last Will khi mất kết nối bất thường hoặc quá 1,5 × keepalive không nhận gói nào.
Không làm: QoS 2, phiên bền (clean session = 0 vẫn coi như phiên mới), xác thực, MQTT 5.

Khung gói MQTT: byte đầu = (loại << 4) | cờ; tiếp theo là "độ dài còn lại" mã hóa 7 bit/byte (bit 8 = còn byte sau,
tối đa 4 byte); chuỗi UTF-8 có tiền tố độ dài 2 byte (big-endian).
"""
import asyncio
import sys
import time

# Windows: khi chuyển hướng đầu ra vào tệp, Python dùng bảng mã ANSI (cp1252) và báo lỗi với chữ Việt -> ép UTF-8.
for _luong in (sys.stdout, sys.stderr):
    if hasattr(_luong, "reconfigure"):
        _luong.reconfigure(encoding="utf-8", errors="replace")

CONNECT, CONNACK, PUBLISH, PUBACK = 1, 2, 3, 4
SUBSCRIBE, SUBACK, UNSUBSCRIBE, UNSUBACK = 8, 9, 10, 11
PINGREQ, PINGRESP, DISCONNECT = 12, 13, 14


def ma_do_dai(n):
    """Mã hóa "độ dài còn lại" (variable byte integer)."""
    kq = bytearray()
    while True:
        b, n = n % 128, n // 128
        kq.append(b | (0x80 if n else 0))
        if not n:
            return bytes(kq)


def chuoi(s):
    b = s.encode("utf-8")
    return len(b).to_bytes(2, "big") + b


def goi(loai, co, than):
    return bytes([(loai << 4) | co]) + ma_do_dai(len(than)) + than


def goi_publish(chu_de, noi_dung, retain=False):
    return goi(PUBLISH, 1 if retain else 0, chuoi(chu_de) + noi_dung)


async def doc_goi(reader):
    """Đọc một gói: (loại, cờ, thân). Hết dữ liệu -> IncompleteReadError."""
    b0 = (await reader.readexactly(1))[0]
    n, nhan = 0, 1
    for _ in range(4):
        b = (await reader.readexactly(1))[0]
        n += (b & 0x7F) * nhan
        nhan *= 128
        if not b & 0x80:
            break
    else:
        raise ValueError("độ dài còn lại quá 4 byte")
    return b0 >> 4, b0 & 0x0F, await reader.readexactly(n) if n else b""


class DocThan:
    """Đọc lần lượt các trường trong thân gói."""

    def __init__(self, b):
        self.b, self.i = b, 0

    def u8(self):
        self.i += 1
        return self.b[self.i - 1]

    def u16(self):
        self.i += 2
        return int.from_bytes(self.b[self.i - 2:self.i], "big")

    def nhi_phan(self):
        n = self.u16()
        self.i += n
        return self.b[self.i - n:self.i]

    def chuoi(self):
        return self.nhi_phan().decode("utf-8")

    def con(self):
        return self.i < len(self.b)

    def phan_con_lai(self):
        return self.b[self.i:]


def khop(bo_loc, chu_de):
    """Chủ đề có khớp bộ lọc không (+ = đúng một cấp, # = mọi cấp còn lại, kể cả cấp cha)."""
    if chu_de.startswith("$") and bo_loc[:1] in ("+", "#"):
        return False
    l, c = bo_loc.split("/"), chu_de.split("/")
    for i, p in enumerate(l):
        if p == "#":
            return True
        if i >= len(c) or (p != "+" and p != c[i]):
            return False
    return len(l) == len(c)


class KetNoiMqtt:
    def __init__(self, broker, reader, writer):
        self.broker, self.reader, self.writer = broker, reader, writer
        self.ma = "?"
        self.dang_ky = set()
        self.di_chuc = None                      # (chủ đề, nội dung, retain) – Last Will
        self.dia_chi = "%s:%s" % writer.get_extra_info("peername")[:2]

    def gui(self, b):
        if not self.writer.is_closing():
            self.writer.write(b)

    async def chay(self):
        b = self.broker
        dong_dep = False
        try:
            loai, _, than = await asyncio.wait_for(doc_goi(self.reader), 10)
            if loai != CONNECT:
                return
            d = DocThan(than)
            ten_gt, muc = d.chuoi(), d.u8()
            co, keepalive = d.u8(), d.u16()
            if ten_gt != "MQTT" or muc != 4:
                self.gui(goi(CONNACK, 0, b"\x00\x01"))      # 1 = không hỗ trợ phiên bản giao thức
                return
            self.ma = d.chuoi() or "tu_dong_%d" % id(self)
            if co & 0x04:
                chu_de, nd = d.chuoi(), d.nhi_phan()
                self.di_chuc = (chu_de, nd, bool(co & 0x20))
            if co & 0x80:
                d.chuoi()                                 # tên người dùng (bỏ qua – lab không xác thực)
            if co & 0x40:
                d.nhi_phan()
            cu = b.ket_noi.get(self.ma)
            if cu is not None:                            # cùng client id: ngắt kết nối cũ (đúng chuẩn)
                cu.writer.close()
            b.ket_noi[self.ma] = self
            self.gui(goi(CONNACK, 0, b"\x00\x00"))
            b.bao(f"{self.ma} ({self.dia_chi}) kết nối, keepalive {keepalive} s")
            han = keepalive * 1.5 if keepalive else None
            while True:
                loai, co, than = await asyncio.wait_for(doc_goi(self.reader), han)
                d = DocThan(than)
                if loai == PUBLISH:
                    qos, retain = (co >> 1) & 3, bool(co & 1)
                    chu_de = d.chuoi()
                    if qos:
                        ma_goi = d.u16()
                        self.gui(goi(PUBACK, 0, ma_goi.to_bytes(2, "big")))
                    b.phat(chu_de, d.phan_con_lai(), retain)
                elif loai == SUBSCRIBE:
                    ma_goi, ma_tra = d.u16(), bytearray()
                    moi = []
                    while d.con():
                        bo_loc, _ = d.chuoi(), d.u8()
                        self.dang_ky.add(bo_loc)
                        moi.append(bo_loc)
                        ma_tra.append(0)                  # cấp QoS 0
                    self.gui(goi(SUBACK, 0, ma_goi.to_bytes(2, "big") + bytes(ma_tra)))
                    for chu_de, nd in list(b.giu_lai.items()):   # tin giữ lại khớp bộ lọc mới
                        if any(khop(f, chu_de) for f in moi):
                            self.gui(goi_publish(chu_de, nd, retain=True))
                elif loai == UNSUBSCRIBE:
                    ma_goi = d.u16()
                    while d.con():
                        self.dang_ky.discard(d.chuoi())
                    self.gui(goi(UNSUBACK, 0, ma_goi.to_bytes(2, "big")))
                elif loai == PINGREQ:
                    self.gui(goi(PINGRESP, 0, b""))
                elif loai == DISCONNECT:
                    dong_dep = True
                    return
        except (asyncio.IncompleteReadError, ConnectionError, asyncio.TimeoutError, ValueError, UnicodeDecodeError):
            pass
        finally:
            if b.ket_noi.get(self.ma) is self:
                del b.ket_noi[self.ma]
                b.bao(f"{self.ma} ngắt kết nối" + ("" if dong_dep else " bất thường"))
                if self.di_chuc and not dong_dep:          # Last Will chỉ phát khi mất kết nối bất thường
                    b.phat(*self.di_chuc)
            self.writer.close()


class MqttBroker:
    def __init__(self, bao=print):
        self.bao = bao
        self.ket_noi = {}                     # client id -> KetNoiMqtt
        self.giu_lai = {}                     # chủ đề -> nội dung giữ lại
        self.noi_bo = []                      # (bộ lọc, hàm(chủ đề, nội dung)) – máy chủ tự theo dõi
        self.so_tin = 0                       # tổng số PUBLISH nhận được
        self.server = None

    def dang_ky_noi_bo(self, bo_loc, ham):
        self.noi_bo.append((bo_loc, ham))

    def phat(self, chu_de, noi_dung, retain=False):
        self.so_tin += 1
        if retain:
            if noi_dung:
                self.giu_lai[chu_de] = noi_dung
            else:
                self.giu_lai.pop(chu_de, None)
        b = goi_publish(chu_de, noi_dung)             # chuyển tới người đăng ký: cờ retain = 0 (chuẩn 3.3.1.3)
        for k in list(self.ket_noi.values()):
            if any(khop(f, chu_de) for f in k.dang_ky):
                k.gui(b)
        for f, ham in self.noi_bo:
            if khop(f, chu_de):
                ham(chu_de, noi_dung)

    async def bat_dau(self, host, cong):
        async def phuc_vu(reader, writer):
            await KetNoiMqtt(self, reader, writer).chay()

        self.server = await asyncio.start_server(phuc_vu, host, cong)
        return self.server


if __name__ == "__main__":                            # chạy riêng: python mqtt_broker.py [host] [cổng]

    async def main():
        b = MqttBroker(lambda t: print(f"[mqtt] {time.strftime('%H:%M:%S')} {t}", flush=True))
        await b.bat_dau(sys.argv[1] if len(sys.argv) > 1 else "0.0.0.0", int(sys.argv[2]) if len(sys.argv) > 2 else 1883)
        print("[mqtt] Lắng nghe", flush=True)
        await asyncio.Event().wait()

    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
