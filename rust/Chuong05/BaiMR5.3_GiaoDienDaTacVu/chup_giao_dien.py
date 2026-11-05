"""Bài mở rộng MR5.3 – Kịch bản minh họa tự động cho giao diện web của bản Rust (chép từ bản Go; không thuộc bài chính).

Playwright điều khiển Chromium không màn hình, thao tác như người dùng: bấm Tải 5 tệp, Kiểm tra cổng,
Tra cứu tên (giới hạn đồng thời 3), bấm Hủy ở tác vụ #2, chụp ảnh lúc nhiều thanh tiến trình đang chạy;
khi xong, bấm Tải 4 tệp rồi bấm "Làm SAI" (gọi đồng bộ, chặn luồng chính của trang), chụp ảnh ngay sau
đó; cuối cùng bấm Tải 4 tệp rồi Hủy tất cả. In thống kê khoảng cách giữa hai nhịp đồng hồ mà trang đo.

Chạy:  python3 chup_giao_dien.py <cổng_giao_diện> [tệp_ảnh]
Cần:   pip install playwright pillow && playwright install chromium
"""
import os
import sys

from PIL import Image
from playwright.sync_api import sync_playwright


def log(text):
    print(f"[trình duyệt] {text}", flush=True)


def main():
    port = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else "ket_qua_gui.png"
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1000, "height": 720})
        page.goto(f"http://127.0.0.1:{port}/")
        page.wait_for_timeout(600)
        page.fill("#n", "5")
        page.click("#btnDl")
        page.click("#btnPorts")
        page.click("#btnDns")
        page.wait_for_timeout(900)
        page.click("#row-2 button")                              # Hủy tác vụ #2
        page.wait_for_timeout(600)
        waiting = page.evaluate("[...document.querySelectorAll('.st')].filter(e => e.textContent === 'chờ lượt').length")
        log(f"Chụp ảnh 1: {page.evaluate('demo.running()')} tác vụ chưa xong, {waiting} đang chờ lượt")
        page.evaluate("demo.pause(true)")                        # không tính thời gian chụp ảnh
        page.locator("#win").screenshot(path=".gui_1.png")
        page.evaluate("demo.pause(false)")
        page.wait_for_function("demo.running() === 0", timeout=20000)
        page.fill("#n", "4")
        page.click("#btnDl")                                     # bốn tệp chạy nền trong lúc làm sai
        page.wait_for_timeout(300)
        log("Bấm Làm SAI (XMLHttpRequest đồng bộ)")
        page.click("#btnBad")                                    # luồng chính của trang bị chặn ~1,2 s
        page.wait_for_timeout(100)
        page.evaluate("demo.pause(true)")                        # không tính thời gian chụp ảnh
        page.locator("#win").screenshot(path=".gui_2.png")
        page.evaluate("demo.pause(false)")
        page.wait_for_function("demo.running() === 0", timeout=20000)
        page.click("#btnDl")
        page.wait_for_timeout(500)
        page.click("#btnCancelAll")
        page.wait_for_function("demo.running() === 0", timeout=20000)
        page.wait_for_timeout(300)
        stats = page.evaluate("demo.stats")
        browser.close()
    for phase, s in stats.items():
        log(f"Nhịp {phase:<19}: {s['n']:>4} nhịp, TB {s['sum'] / s['n']:6.1f} ms, tối đa {s['max']:6.0f} ms")
    a, b = Image.open(".gui_1.png"), Image.open(".gui_2.png")
    img = Image.new("RGB", (max(a.width, b.width) + 12, a.height + b.height + 18), "#d0d4dc")
    img.paste(a, (6, 6))
    img.paste(b, (6, a.height + 12))
    img.save(out)
    os.remove(".gui_1.png")
    os.remove(".gui_2.png")
    log(f"Đã chụp giao diện: {out}")


if __name__ == "__main__":
    main()
