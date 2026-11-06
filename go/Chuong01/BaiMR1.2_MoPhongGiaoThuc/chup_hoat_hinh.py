"""Bài mở rộng MR1.2 – Chụp ảnh tệp hoạt hình HTML bằng Playwright (chỉ dùng cho kịch bản minh họa).

Mở tệp HTML do mo_phong.py sinh ra trong Chromium không màn hình, tua tới hai thời điểm đáng chú ý
(TCP lúc mất gói và có ACK trùng; multicast lúc gói tới hai thành viên ở LAN B; bất đồng bộ và phối hợp
lúc 62 ms – so sánh hàng luồng của máy chủ H5), dừng hoạt hình,
chụp lại và ghép hai ảnh thành một tệp PNG.

Chạy:  python3 chup_hoat_hinh.py <tệp.html> [tệp_ảnh.png]
Cần:   pip install playwright pillow && playwright install chromium
"""
import os
import pathlib
import sys

from PIL import Image
from playwright.sync_api import sync_playwright

SHOTS = [("tcp", 150000), ("multicast", 102600),    # (kịch bản, thời điểm µs)
         ("bat_dong_bo", 62000), ("phoi_hop", 62000)]


def main():
    html = os.path.abspath(sys.argv[1])
    out = sys.argv[2] if len(sys.argv) > 2 else "ket_qua_gui.png"
    files = []
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1280, "height": 900})
        errors = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto(pathlib.Path(html).as_uri())          # file:///... đúng cả trên Windows (C:\...)
        page.wait_for_timeout(300)
        for i, (kb, t) in enumerate(SHOTS):
            page.evaluate(f"seek('{kb}', {t}, true)")
            page.wait_for_timeout(200)
            clock = page.text_content("#clock")
            flying = page.evaluate(f"DATA.runs.find(r => r.id === '{kb}').hops"
                                   f".filter(h => h[0] <= {t} && {t} <= h[1]).length")
            print(f"[trình duyệt] {kb}: {clock}, {flying} gói đang trên đường truyền")
            path = f".shot_{i}.png"
            page.locator("body").screenshot(path=path)
            files.append(path)
        browser.close()
    if errors:
        print("[trình duyệt] Lỗi JavaScript:", errors)
    imgs = [Image.open(f) for f in files]
    W = max(i.width for i in imgs)
    canvas = Image.new("RGB", (W, sum(i.height for i in imgs) + 8 * (len(imgs) - 1)), "#d0d4dc")
    y = 0
    for im in imgs:
        canvas.paste(im, (0, y))
        y += im.height + 8
    canvas.save(out)
    for f in files:
        os.remove(f)
    print(f"[trình duyệt] Đã chụp hoạt hình: {out}")


if __name__ == "__main__":
    main()
