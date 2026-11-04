"""Bài 5.3 – Kịch bản minh họa tự động cho lookup_gui.py (chạy dưới xvfb-run).

Kịch bản thao tác trên cửa sổ thật như người dùng: bấm Kết nối tới cổng không có server
(lỗi REFUSED), bấm Kết nối tới đúng cổng, nhập "TCP" và bấm Tra cứu. Trong lúc chờ server
trả lời chậm, kịch bản đếm số lần đồng hồ giao diện được vẽ lại và chụp cửa sổ; sau khi có
kết quả, chụp cửa sổ lần nữa. Hai ảnh được ghép thành ket_qua_gui.png (ImageMagick). Cuối cùng tra cứu
"XYZ" để thấy phản hồi 404 hiển thị dạng "(404) Không có thuật ngữ".

Chạy:  xvfb-run -a python3 demo_gui.py <host> <port> <cổng_không_có_server> [tệp_ảnh]
"""
import asyncio
import subprocess
import sys
import tkinter as tk

from lookup_gui import LookupWindow, run_gui


def log(text):
    print(f"[gui] {text}", flush=True)


def snapshot(win, path):
    win.root.update()
    subprocess.run(["import", "-window", win.root.wm_frame(), path], check=True)


async def wait_until(cond, timeout=10.0):
    for _ in range(int(timeout / 0.05)):
        if cond():
            return
        await asyncio.sleep(0.05)
    raise TimeoutError("Kịch bản chờ quá lâu")


async def scenario(win, port, bad_port, out):
    await asyncio.sleep(0.5)
    win.txt_port.delete(0, "end"); win.txt_port.insert(0, str(bad_port))
    win.btn_connect.invoke()                              # bấm Kết nối (cổng sai)
    await wait_until(lambda: win.btn_connect["state"] == "normal")
    log(f"Kết nối cổng {bad_port}: {win.lbl_status['text']}")
    win.txt_port.delete(0, "end"); win.txt_port.insert(0, str(port))
    win.btn_connect.invoke()                              # bấm Kết nối (cổng đúng)
    await wait_until(lambda: win.btn_lookup["state"] == "normal")
    log(f"Kết nối cổng {port}: {win.lbl_status['text']}")
    win.txt_term.insert(0, "TCP")
    t0 = win.ticks
    win.btn_lookup.invoke()                               # bấm Tra cứu
    await asyncio.sleep(0.6)
    log(f"Đang chờ phản hồi: trạng thái '{win.lbl_status['text']}', "
        f"nút Tra cứu {win.btn_lookup['state']}")
    snapshot(win, ".gui_1.png")
    await wait_until(lambda: win.lbl_status["text"] != "Đang tra cứu...")
    log(f"Kết quả: {win.txt_result['text']} – trạng thái '{win.lbl_status['text']}'")
    log(f"Trong lúc chờ, đồng hồ giao diện được vẽ lại {win.ticks - t0} lần (không đứng hình)")
    snapshot(win, ".gui_2.png")
    subprocess.run(["convert", ".gui_1.png", ".gui_2.png", "-bordercolor", "#f5f5f5",
                    "-border", "8", "+append", out], check=True)
    subprocess.run(["rm", "-f", ".gui_1.png", ".gui_2.png"])
    log(f"Đã chụp cửa sổ: {out}")
    win.txt_term.delete(0, "end"); win.txt_term.insert(0, "XYZ")
    win.btn_lookup.invoke()                               # tra cứu thuật ngữ không có: phản hồi 404
    await asyncio.sleep(0.3)
    await wait_until(lambda: win.btn_lookup["state"] == "normal")
    log(f"Tra cứu 'XYZ': {win.txt_result['text']} – trạng thái '{win.lbl_status['text']}'")
    win.close()


async def main():
    host, port, bad = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    out = sys.argv[4] if len(sys.argv) > 4 else "ket_qua_gui.png"
    win = LookupWindow(tk.Tk(), host, port)
    await asyncio.gather(run_gui(win), scenario(win, port, bad, out))


if __name__ == "__main__":
    asyncio.run(main())
