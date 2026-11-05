"""Bài mở rộng MR5.5 – Máy tạo tải bất đồng bộ: N yêu cầu đồng thời cho từng chế độ, bảng so sánh.

Mỗi yêu cầu một kết nối TCP; 16 coroutine chạy cùng lúc trên một vòng lặp (asyncio.gather).
Sau mỗi đợt, client gửi THONGKE để lấy số đo phía server (trễ nhịp tim, cảm biến, nhóm tính).
"""
import asyncio
import time

from chung import GIOI_HAN, N_YC, NHOM_TINH, SO_BYTE, SO_VONG, du_lieu, say

CHE_DO = [("chan", "A. CPU ngay trên vòng lặp"),
          ("tach", "B. đẩy sang nhóm luồng"),
          ("gioihan", "C. nhóm luồng + áp lực ngược")]
NGAN = ["A. chặn vòng lặp", "B. nhóm luồng", "C. + áp lực ngược"]   # nhãn ngắn trong bảng


async def gui(host, port, line, data=b""):
    reader, writer = await asyncio.open_connection(host, port)
    writer.write(line.encode() + b"\n" + data)
    await writer.drain()
    tra_loi = (await reader.readline()).decode().strip()
    writer.close()
    await writer.wait_closed()
    return tra_loi


async def chay(host, port):
    data = du_lieu(SO_BYTE)
    say(f"Máy tạo tải: {N_YC} yêu cầu đồng thời mỗi đợt, mỗi yêu cầu {SO_BYTE // 1024} KB, "
        f"băm SHA-256 {SO_VONG} vòng (≈ {SO_BYTE * SO_VONG / 1e6:.0f} MB dữ liệu băm)")
    bang = []
    mau_hex = None
    for che_do, nhan in CHE_DO:
        await gui(host, port, "DATLAI")
        t = time.perf_counter()
        kq = await asyncio.gather(*[gui(host, port, f"BAM {k} {che_do} {SO_BYTE} {SO_VONG} 10000", data)
                                    for k in range(1, N_YC + 1)])
        dt = time.perf_counter() - t
        ok = sum(1 for x in kq if x.startswith("OK"))
        hex_set = {x.split()[2] for x in kq if x.startswith("OK")}
        mau_hex = mau_hex or next(iter(hex_set), "")
        tk = (await gui(host, port, f"THONGKE {nhan}")).split()
        say(f"Đợt {nhan}: {ok}/{N_YC} OK, {dt:.2f} s, {N_YC / dt:.1f} yc/s, "
            f"{len(hex_set)} giá trị băm khác nhau")
        bang.append((nhan, dt, tk))
    say()
    say(f"  {'Chế độ':<18}{'Thời gian':>10}{'yc/s':>7}{'Trễ nhịp':>10}{'Cảm biến':>10}{'Nhóm':>6}")
    for ngan, (nhan, dt, tk) in zip(NGAN, bang):
        say(f"  {ngan:<18}{dt:>8.2f} s{N_YC / dt:>7.1f}{float(tk[1]):>7.1f} ms{float(tk[3]):>7.1f} ms{int(tk[4]):>6}")
    say("  (Trễ nhịp, Cảm biến: độ trễ tối đa của nhịp tim 10 ms và của")
    say("   mẫu cảm biến; Nhóm: số việc ở nhóm tính cùng lúc, tối đa)")
    say(f"Băm kiểm tra (16 hex đầu): {mau_hex} – giống nhau ở mọi bản ngôn ngữ")
    say("Nhận xét: A chặn vòng lặp → nhịp tim, mẫu cảm biến và mọi kết nối khác đều đứng chờ; B, C giữ")
    say(f"  vòng lặp thông suốt; C chỉ cho {GIOI_HAN} việc vào nhóm {NHOM_TINH} luồng, phần còn lại chờ")
    say("  bất đồng bộ ở cổng vào – hàng đợi của nhóm luồng không phình theo tải.")

    say()
    say("(d) Hạn chót và hủy xuyên ranh giới async → luồng (chế độ tach):")
    for ma, so_vong, han in [(101, SO_VONG, 1000), (102, SO_VONG * 8, 30)]:
        t = time.perf_counter()
        tl = await gui(host, port, f"BAM {ma} tach {SO_BYTE} {so_vong} {han}", data)
        say(f"  yêu cầu {ma} ({so_vong} vòng, hạn {han} ms): {tl.split()[0]} sau "
            f"{(time.perf_counter() - t) * 1000:.0f} ms")
    say()
    say("(c) Luồng nền giao kết quả vào vòng lặp sự kiện:")
    for ma in (201, 202):
        tl = (await gui(host, port, f"CAMBIEN {ma}")).split()
        say(f"  CAMBIEN {ma} → giá trị {tl[2]}, future do luồng {tl[3]} hoàn tất")


def run(host, port):
    asyncio.run(chay(host, int(port)))
