"""Bài mở rộng MR5.5 – Server bất đồng bộ (asyncio) phối hợp với nhóm luồng và luồng nền.

Các luồng của tiến trình server:
  vòng-lặp   – luồng chính chạy vòng lặp sự kiện asyncio: nhận kết nối, đọc/ghi socket, nhịp tim 10 ms;
  tính-1, -2 – nhóm luồng tính (ThreadPoolExecutor riêng) cho việc băm SHA-256 (CPU);
  chặn-k     – nhóm luồng mặc định của vòng lặp (asyncio.to_thread) cho hàm thư viện cũ kiểu chặn;
  cảm-biến   – một luồng hệ điều hành chạy vòng lặp chặn của riêng nó, đẩy mẫu đo VÀO vòng lặp sự kiện
               bằng loop.call_soon_threadsafe (chỉ hàm này an toàn khi gọi từ luồng khác).
Ranh giới async ↔ luồng:
  async → luồng : loop.run_in_executor / asyncio.to_thread trả về awaitable; await không chặn vòng lặp;
  luồng → async : loop.call_soon_threadsafe(cb, ...) hoặc asyncio.run_coroutine_threadsafe(coro, loop);
  hủy, hạn chót : asyncio.wait_for hủy coroutine → coroutine bắt CancelledError, bật threading.Event
                  mà luồng tính kiểm tra mỗi vòng → luồng tự dừng (không thể "giết" một luồng đang chạy).
"""
import asyncio
import concurrent.futures as cf
import itertools
import queue
import threading
import time

from chung import (CAM_BIEN_MS, GIOI_HAN, NHIP_MS, NHOM_TINH, bam, log, say, tra_cuu_cu)


def dat_ten(tien_to):
    """initializer cho ThreadPoolExecutor: đặt tên luồng dễ đọc tính-1, tính-2, ..."""
    dem = itertools.count(1)
    return lambda: setattr(threading.current_thread(), "name", f"{tien_to}-{next(dem)}")


def tom_tat(ten):
    """Danh sách tên luồng; quá 8 tên thì chỉ in 3 tên đầu và tổng số."""
    ds = sorted(ten)
    if not ds:
        return "–"
    return ", ".join(ds) if len(ds) <= 8 else ", ".join(ds[:3]) + f", … ({len(ds)} luồng)"


class MayChu:
    def __init__(self, loop):
        self.loop = loop
        self.nhom_tinh = cf.ThreadPoolExecutor(NHOM_TINH, initializer=dat_ten("tính"))
        loop.set_default_executor(cf.ThreadPoolExecutor(8, initializer=dat_ten("chặn")))
        self.cong_vao = asyncio.Semaphore(GIOI_HAN)    # semaphore BẤT ĐỒNG BỘ: chờ không chặn vòng lặp
        self.yc_cam_bien = queue.Queue()                # vòng lặp → luồng cảm biến (hàng đợi an toàn luồng)
        self.dat_lai()

    def dat_lai(self):
        self.tre_nhip = 0.0
        self.so_mau, self.tre_mau = 0, 0.0
        self.o_nhom = self.o_nhom_max = 0
        self.luong = {"nhận/trả lời": set(), "tra cứu cũ": set(), "băm SHA-256": set()}
        self.so_yc = 0

    # ---------- nhịp tim: đo độ trễ của vòng lặp sự kiện ----------
    async def nhip_tim(self):
        while True:
            t = time.perf_counter()
            await asyncio.sleep(NHIP_MS / 1000)
            tre = (time.perf_counter() - t) * 1000 - NHIP_MS   # thức dậy muộn bao nhiêu ms
            self.tre_nhip = max(self.tre_nhip, tre)

    # ---------- luồng cảm biến: luồng HĐH → vòng lặp sự kiện ----------
    def cam_bien(self):
        k = 0
        while True:
            try:   # chờ yêu cầu đọc ngay từ vòng lặp, tối đa một chu kỳ
                fut = self.yc_cam_bien.get(timeout=CAM_BIEN_MS / 1000)
                k += 1
                gia_tri = (k * 37) % 100
                # Future của asyncio KHÔNG an toàn luồng: phải nhờ vòng lặp gọi set_result giúp
                self.loop.call_soon_threadsafe(fut.set_result, (gia_tri, threading.current_thread().name))
            except queue.Empty:
                k += 1
                self.loop.call_soon_threadsafe(self.nhan_mau, k, time.perf_counter())

    def nhan_mau(self, k, t_tao):
        """Chạy TRÊN vòng lặp (do call_soon_threadsafe lên lịch): không cần khóa."""
        self.so_mau += 1
        self.tre_mau = max(self.tre_mau, (time.perf_counter() - t_tao) * 1000)

    # ---------- xử lý một yêu cầu BAM ----------
    def vao_nhom(self):
        self.o_nhom += 1
        self.o_nhom_max = max(self.o_nhom_max, self.o_nhom)

    async def tach(self, ma, data, so_vong):
        """B, C: hàm chặn → nhóm luồng chặn; băm → nhóm luồng tính; vòng lặp chỉ await."""
        huy = threading.Event()                         # cờ hủy chia sẻ với luồng tính
        try:
            ten_cu = await asyncio.to_thread(tra_cuu_cu, ma)
            self.luong["tra cứu cũ"].add(ten_cu)
            self.vao_nhom()
            f = self.nhom_tinh.submit(bam, data, so_vong, huy)
            # khi luồng tính xong THẬT (kể cả sau khi bị hủy), báo về vòng lặp để giảm bộ đếm
            f.add_done_callback(lambda _: self.loop.call_soon_threadsafe(self.ra_nhom, ma, f))
            return await asyncio.wrap_future(f)
        except asyncio.CancelledError:
            huy.set()                                   # hủy đi xuyên ranh giới: báo luồng tính dừng
            raise

    def ra_nhom(self, ma, f):
        self.o_nhom -= 1
        if f.cancelled():                               # bị hủy khi còn nằm trong hàng đợi của nhóm
            say(f"[server] BAM {ma}: việc bị hủy trước khi luồng tính bắt đầu")
            return
        hex16, ten = f.result()
        if hex16 is None:
            say(f"[server] BAM {ma}: luồng tính thấy cờ hủy → dừng ở vòng {ten}")

    async def xu_ly_bam(self, ma, che_do, data, so_vong, han_ms, ghi):
        if che_do == "chan":                            # A. SAI: chặn vòng lặp sự kiện
            self.luong["tra cứu cũ"].add(tra_cuu_cu(ma))
            hex16, ten = bam(data, so_vong)
        elif che_do == "tach":                          # B. đẩy sang nhóm luồng
            hex16, ten = await asyncio.wait_for(self.tach(ma, data, so_vong), han_ms / 1000)
        else:                                           # C. + áp lực ngược
            async with self.cong_vao:                   # quá GIOI_HAN yêu cầu thì CHỜ (bất đồng bộ) ở đây
                hex16, ten = await asyncio.wait_for(self.tach(ma, data, so_vong), han_ms / 1000)
        self.luong["băm SHA-256"].add(ten)
        if ghi:
            log(f"BAM {ma}: băm xong trên {ten}, trả lời OK")
        return f"OK {ma} {hex16}"

    async def phuc_vu(self, reader, writer):
        self.luong["nhận/trả lời"].add(threading.current_thread().name)
        parts = (await reader.readline()).decode().split()
        if not parts:
            writer.close()
            return
        if parts[0] == "BAM":
            ma, che_do, n, so_vong, han_ms = parts[1], parts[2], int(parts[3]), int(parts[4]), int(parts[5])
            data = await reader.readexactly(n)
            ghi = ma in ("1", "101", "102")             # chỉ ghi dòng thời gian cho vài yêu cầu tiêu biểu
            if ghi:
                log(f"BAM {ma} ({che_do}): nhận {n} byte, {so_vong} vòng, hạn {han_ms} ms")
            self.so_yc += 1
            try:
                tra_loi = await self.xu_ly_bam(ma, che_do, data, so_vong, han_ms, ghi)
            except TimeoutError:
                log(f"BAM {ma}: quá hạn {han_ms} ms → trả HETGIO, bật cờ hủy cho luồng tính")
                tra_loi = f"HETGIO {ma}"
        elif parts[0] == "CAMBIEN":
            fut = self.loop.create_future()
            self.yc_cam_bien.put(fut)                   # nhờ luồng cảm biến đọc ngay
            gia_tri, ten = await asyncio.wait_for(fut, 1.0)
            tra_loi = f"CAMBIEN {parts[1]} {gia_tri} {ten}"
            log(f"CAMBIEN {parts[1]}: future được hoàn tất từ luồng {ten}, giá trị {gia_tri}")
        elif parts[0] == "DATLAI":
            self.dat_lai()
            tra_loi = "OK"
        else:  # THONGKE <nhãn>
            nhan = " ".join(parts[1:])
            say(f"[server] đợt \"{nhan}\": {self.so_yc} yêu cầu")
            say(f"  {'giai đoạn':<15}luồng đã chạy")
            for gd, ten in self.luong.items():
                say(f"  {gd:<15}{tom_tat(ten)}")
            say(f"  trễ nhịp tối đa {self.tre_nhip:.1f} ms; nhóm tính tối đa {self.o_nhom_max} việc")
            say(f"  cảm biến {self.so_mau} mẫu, trễ giao tối đa {self.tre_mau:.1f} ms")
            tra_loi = f"TK {self.tre_nhip:.1f} {self.so_mau} {self.tre_mau:.1f} {self.o_nhom_max}"
        writer.write((tra_loi + "\n").encode())
        await writer.drain()
        writer.close()


async def chay(host, port):
    threading.current_thread().name = "vòng-lặp"
    mc = MayChu(asyncio.get_running_loop())
    srv = await asyncio.start_server(mc.phuc_vu, host, int(port), backlog=128)
    threading.Thread(target=mc.cam_bien, name="cảm-biến", daemon=True).start()
    asyncio.create_task(mc.nhip_tim())
    say(f"[server] lắng nghe {host}:{port}; vòng lặp sự kiện trên luồng \"vòng-lặp\", nhóm tính {NHOM_TINH} luồng,")
    say(f"  nhịp tim {NHIP_MS} ms, luồng cảm biến mỗi {CAM_BIEN_MS} ms, chế độ C giới hạn {GIOI_HAN} yêu cầu")
    say("Đặc thù Python: hashlib nhả GIL khi băm khối ≥ 2 KB nên nhóm luồng tính chạy song song được;")
    say("  việc CPU thuần Python thì phải dùng loop.run_in_executor(ProcessPoolExecutor(), ...) (xem MR5.2).")
    async with srv:
        await srv.serve_forever()


def run(host, port):
    try:
        asyncio.run(chay(host, port))
    except KeyboardInterrupt:
        pass
