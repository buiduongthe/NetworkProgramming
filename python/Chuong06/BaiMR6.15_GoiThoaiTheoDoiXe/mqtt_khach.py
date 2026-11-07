"""Bài mở rộng MR6.15 – MQTT client tối giản (MQTT 3.1.1, QoS 0) dùng cho tài xế giả lập và khách theo dõi.

Cùng cách mã hóa gói với mqtt_broker.py; app Android có bản Kotlin tương đương (MqttKhach.kt).
"""
import asyncio

from mqtt_broker import (CONNACK, CONNECT, DISCONNECT, PINGREQ, PUBLISH, SUBACK, SUBSCRIBE, DocThan, chuoi, doc_goi,
                         goi)


class MqttKhach:
    def __init__(self, ma, khi_nhan=None, keepalive=30):
        self.ma, self.khi_nhan, self.keepalive = ma, khi_nhan, keepalive
        self.reader = self.writer = None
        self.ma_goi = 0
        self.da_dang_ky = asyncio.Event()
        self._doc = self._ping = None

    async def ket_noi(self, host, cong, di_chuc=None):
        """di_chuc = (chủ đề, nội dung bytes, retain): Last Will – broker phát khi mình mất kết nối bất thường."""
        self.reader, self.writer = await asyncio.open_connection(host, cong)
        co = 0x02                                         # clean session
        than = chuoi("MQTT") + bytes([4])
        tai = chuoi(self.ma)
        if di_chuc:
            co |= 0x04 | (0x20 if di_chuc[2] else 0)
            tai += chuoi(di_chuc[0]) + len(di_chuc[1]).to_bytes(2, "big") + di_chuc[1]
        than += bytes([co]) + self.keepalive.to_bytes(2, "big") + tai
        self.writer.write(goi(CONNECT, 0, than))
        loai, _, t = await asyncio.wait_for(doc_goi(self.reader), 5)
        if loai != CONNACK or t[1] != 0:
            raise ConnectionError(f"broker từ chối: mã {t[1] if len(t) > 1 else '?'}")
        self._doc = asyncio.create_task(self._vong_doc())
        self._ping = asyncio.create_task(self._vong_ping())

    async def _vong_doc(self):
        try:
            while True:
                loai, co, than = await doc_goi(self.reader)
                if loai == PUBLISH:
                    d = DocThan(than)
                    chu_de = d.chuoi()
                    if (co >> 1) & 3:
                        d.u16()
                    if self.khi_nhan:
                        self.khi_nhan(chu_de, d.phan_con_lai(), bool(co & 1))
                elif loai == SUBACK:
                    self.da_dang_ky.set()
        except (asyncio.IncompleteReadError, ConnectionError):
            pass

    async def _vong_ping(self):
        while True:
            await asyncio.sleep(self.keepalive * 0.8)
            self.writer.write(goi(PINGREQ, 0, b""))

    def dang_ky(self, *bo_loc):
        self.ma_goi += 1
        than = self.ma_goi.to_bytes(2, "big") + b"".join(chuoi(f) + b"\x00" for f in bo_loc)
        self.writer.write(goi(SUBSCRIBE, 2, than))

    def phat(self, chu_de, noi_dung, retain=False):
        self.writer.write(goi(PUBLISH, 1 if retain else 0, chuoi(chu_de) + noi_dung))

    async def dong(self, dep=True):
        """dep=False: đóng TCP không gửi DISCONNECT (giả lập mất sóng) -> broker phát Last Will."""
        for t in (self._doc, self._ping):
            if t:
                t.cancel()
        if self.writer:
            if dep:
                self.writer.write(goi(DISCONNECT, 0, b""))
                await self.writer.drain()
            self.writer.close()
