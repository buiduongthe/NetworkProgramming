"""Bài 5.3 – Kịch bản minh họa tự động cho giao diện web của bản Go (không thuộc mã giả).

Kịch bản dùng Playwright điều khiển trình duyệt Chromium không màn hình, thao tác như người dùng:
bấm Kết nối tới cổng không có server (lỗi REFUSED), bấm Kết nối tới đúng cổng, nhập "TCP" và bấm
Tra cứu. Trong lúc chờ server trả lời chậm, kịch bản đếm số lần đồng hồ giao diện được vẽ lại và chụp
cửa sổ; sau khi có kết quả, chụp lần nữa. Hai ảnh được ghép thành ket_qua_gui.png (Pillow).

Chạy:  python3 chup_giao_dien.py <cổng_giao_diện> <cổng_server> <cổng_không_có_server> [tệp_ảnh]
Cần:   pip install playwright pillow && playwright install chromium
"""
import os
import sys

from PIL import Image
from playwright.sync_api import sync_playwright


def log(text):
    print(f"[trình duyệt] {text}", flush=True)


def main():
    ui_port, port, bad = sys.argv[1], sys.argv[2], sys.argv[3]
    out = sys.argv[4] if len(sys.argv) > 4 else "ket_qua_gui.png"
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 640, "height": 200})
        page.goto(f"http://127.0.0.1:{ui_port}/?host=127.0.0.1&port={bad}")
        page.wait_for_timeout(500)
        page.click("#btnConnect")                                   # bấm Kết nối (cổng sai)
        page.wait_for_function("!document.getElementById('btnConnect').disabled")
        log(f"Kết nối cổng {bad}: {page.text_content('#lblStatus')}")
        page.fill("#txtPort", port)
        page.click("#btnConnect")                                   # bấm Kết nối (cổng đúng)
        page.wait_for_function("!document.getElementById('btnLookup').disabled")
        log(f"Kết nối cổng {port}: {page.text_content('#lblStatus')}")
        page.fill("#txtTerm", "TCP")
        t0 = page.evaluate("ticks")
        page.click("#btnLookup")                                    # bấm Tra cứu
        page.wait_for_timeout(600)
        state = "disabled" if page.is_disabled("#btnLookup") else "normal"
        log(f"Đang chờ phản hồi: trạng thái '{page.text_content('#lblStatus')}', nút Tra cứu {state}")
        page.locator("#win").screenshot(path=".gui_1.png")
        page.wait_for_function("document.getElementById('lblStatus').textContent !== 'Đang tra cứu...'")
        log(f"Kết quả: {page.text_content('#txtResult')} – trạng thái '{page.text_content('#lblStatus')}'")
        log(f"Trong lúc chờ, đồng hồ giao diện được vẽ lại {page.evaluate('ticks') - t0} lần (không đứng hình)")
        page.locator("#win").screenshot(path=".gui_2.png")
        page.goto("about:blank")                                    # rời trang (đóng cửa sổ): trang gửi /api/close
        page.wait_for_timeout(500)
        browser.close()
    a, b = Image.open(".gui_1.png"), Image.open(".gui_2.png")
    img = Image.new("RGB", (a.width + b.width + 24, max(a.height, b.height) + 16), "#f5f5f5")
    img.paste(a, (8, 8))
    img.paste(b, (a.width + 16, 8))
    img.save(out)
    os.remove(".gui_1.png")
    os.remove(".gui_2.png")
    log(f"Đã chụp cửa sổ: {out}")


if __name__ == "__main__":
    main()
