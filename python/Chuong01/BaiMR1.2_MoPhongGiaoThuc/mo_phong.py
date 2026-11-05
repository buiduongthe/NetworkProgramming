"""Bài mở rộng MR1.2 – Mô phỏng giao thức và kiểu truyền: TCP, UDP, unicast, broadcast, multicast, anycast.

Bài mở rộng, không có trong giáo trình; minh họa trực quan Chương 1 (mô hình TCP/IP, tầng giao vận,
các kiểu địa chỉ) trước khi lập trình socket thật ở các chương sau. Toàn bộ chạy trong bộ mô phỏng
sự kiện rời rạc (mang.py): không mở socket, không cần quyền quản trị, kết quả tất định theo seed.

Cách dùng:
    python3 mo_phong.py <kịch_bản> [--seed N] [--loss P] [--doan N] [--mat 20,20] [--html tệp.html]
    kịch_bản: tcp | udp | unicast | broadcast | multicast | anycast            (giao thức, kiểu truyền)
              dong_bo | da_luong | nhom_luong | bat_dong_bo | phoi_hop   (bên trong máy chủ)
              so_sanh (bảng so sánh 5 mô hình máy chủ) | tat_ca
    --seed  hạt giống bộ sinh ngẫu nhiên (mặc định 7)
    --loss  xác suất mất gói trên liên kết R2–R3, 0..1 (mặc định: 0; UDP: 0.2)
    --doan  số đoạn dữ liệu TCP (mặc định 40)
    --mat   các đoạn TCP bị ép mất; lặp số đoạn để mất nhiều lần (mặc định 20,20)
    --html  ghi tệp HTML hoạt hình tự chứa (JavaScript thuần, không cần mạng)
"""
import argparse
import json
import os

import kich_ban
import may_chu
from mang import LINKS, NODES, Net
from tcp import MSS, TcpScenario

TITLES = {
    "tcp": "TCP – bắt tay, cửa sổ trượt, cwnd, mất gói, đóng FIN",
    "udp": "UDP – datagram, mất gói, sai thứ tự, không gửi lại",
    "unicast": "Unicast – một nguồn tới một đích",
    "broadcast": "Broadcast – gửi tràn trong LAN, dừng ở router",
    "multicast": "Multicast – IGMP, chỉ giao cho thành viên nhóm",
    "anycast": "Anycast – máy phục vụ gần nhất, dự phòng khi hỏng",
    **may_chu.TITLES,
}
ORDER = list(TITLES)


def run_one(name, a):
    print(f"=== Kịch bản {name} (seed {a.seed}) ===")
    print(TITLES[name])
    net = Net(a.seed)
    loss_set = a.loss >= 0
    if loss_set:
        net.links[("R2", "R3")].loss = int(a.loss * 1000 + 0.5)
    extra = {}
    if name in may_chu.TITLES:
        extra = may_chu.chay(net, name)
        print()
        return {"id": name, "title": TITLES[name], "hops": net.hops, "notes": net.notes,
                "changes": net.changes, **extra}
    if name == "tcp":
        drops = [int(x) for x in a.mat.split(",") if x.strip()]
        print(f"Truyền {a.doan} đoạn × {MSS} byte từ H1 tới H5; ép mất đoạn: "
              f"{', '.join('#' + str(d) for d in drops) or 'không'}")
        sc = TcpScenario(net, a.doan, drops)
        sc.start()
        net.run()
        sc.report()
        extra["cwnd"] = sc.cwnd_log
    else:
        if name == "udp":
            kich_ban.udp(net, loss_set)
        else:
            getattr(kich_ban, name)(net)
    print()
    net.summary()
    print()
    return {"id": name, "title": TITLES[name], "hops": net.hops, "notes": net.notes,
            "changes": net.changes, **extra}


def write_html(path, runs):
    data = {"nodes": [{"name": n[0], "kind": n[1], "ip": n[2], "x": n[4], "y": n[5]} for n in NODES],
            "links": [{"a": l[0], "b": l[1], "cost": l[2]} for l in LINKS],
            "runs": runs}
    here = os.path.dirname(os.path.abspath(__file__))
    tpl = open(os.path.join(here, "khung_hoat_hinh.html"), encoding="utf-8").read()
    js = json.dumps(data, ensure_ascii=False, separators=(",", ":")).replace("</", "<\\/")
    with open(path, "w", encoding="utf-8") as f:
        f.write(tpl.replace("{{.Data}}", js))
    print(f"Đã ghi hoạt hình HTML: {path}")


def main():
    ap = argparse.ArgumentParser(description="Mô phỏng giao thức mạng theo sự kiện rời rạc")
    ap.add_argument("kich_ban", choices=ORDER + ["so_sanh", "tat_ca"])
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--loss", type=float, default=-1)
    ap.add_argument("--doan", type=int, default=40)
    ap.add_argument("--mat", default="20,20")
    ap.add_argument("--html", default="")
    a = ap.parse_args()
    if a.kich_ban == "so_sanh":
        may_chu.so_sanh([(m, may_chu.chi_so(Net(a.seed), m)) for m in may_chu.TITLES])
        return
    names = ORDER if a.kich_ban == "tat_ca" else [a.kich_ban]
    runs = [run_one(n, a) for n in names]
    if a.html:
        write_html(a.html, runs)


if __name__ == "__main__":
    main()
