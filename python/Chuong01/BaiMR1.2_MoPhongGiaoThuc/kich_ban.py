"""Bài mở rộng MR1.2 – Các kịch bản UDP, unicast, broadcast, multicast, anycast trên bộ mô phỏng."""
from mang import Packet, fmt_t

MS = 1000          # 1 ms = 1000 µs


def show_path(net, a, b):
    p = net.path(a, b)
    print(f"  Đường {a}→{b}: {' → '.join(p)} (chi phí {net.dist[a][b]})")


# ---------------- UDP ----------------
def udp(net, loss_set):
    """H2 gửi 12 datagram tới H6, cách nhau 10 ms. Liên kết R2–R3 mất gói ngẫu nhiên và có trễ
    dao động (jitter) nên datagram có thể mất hoặc đến sai thứ tự; UDP không gửi lại."""
    L = net.links[("R2", "R3")]
    if not loss_set:
        L.loss = 200                      # mặc định 20 %
    L.jitter = 15 * MS                    # trễ thêm ngẫu nhiên 0..15 ms
    net.hop_trace = False
    print(f"Liên kết R2–R3: mất gói {L.loss // 10}.{L.loss % 10} %, trễ dao động 0–15 ms")
    show_path(net, "H2", "H6")
    got, state = [], {"expect": 1, "late": 0}

    def deliver(node, pkt):
        n = pkt.data["n"]
        got.append(n)
        if n == state["expect"]:
            note = "đúng thứ tự"
        elif n > state["expect"]:
            miss = ", ".join(f"#{i}" for i in range(state["expect"], n))
            note = f"đến sớm, đang thiếu {miss}"
        else:
            note = "ĐẾN MUỘN – sai thứ tự"
            state["late"] += 1
        state["expect"] = max(state["expect"], n + 1)
        net.log(f"H6 nhận datagram #{n:<2} {note}", node)

    net.on_deliver = deliver
    for i in range(1, 13):
        def go(i=i):
            net.log(f"H2 gửi datagram #{i} (200 byte)", "H2")
            net.send("H2", Packet("uni", "UDP", "H2", "H6", f"UDP #{i}", 200, {"n": i}))
        net.at((i - 1) * 10 * MS, go)
    net.run()
    lost = [f"#{i}" for i in range(1, 13) if i not in got]
    print()
    print(f"Tóm tắt UDP: gửi 12, nhận {len(got)}, mất {len(lost)} ({', '.join(lost) or 'không'}),"
          f" sai thứ tự {state['late']}")
    print("  UDP không xác nhận, không gửi lại, không sắp xếp lại:")
    print("  ứng dụng tự xử lý (đánh số thứ tự, bỏ qua hoặc yêu cầu lại).")


# ---------------- Unicast ----------------
def unicast(net):
    print("Bảng định tuyến của R1 (đích → chặng kế, chi phí):")
    for h in ["H2", "H5"]:
        print(f"  {h:<3} → {net.nh['R1'][h]:<3} chi phí {net.dist['R1'][h]}")
    show_path(net, "H1", "H2")
    show_path(net, "H1", "H5")

    def deliver(node, pkt):
        net.log(f"{node} nhận \"{pkt.data['msg']}\" từ {pkt.src} (TTL còn {pkt.ttl})", node)
        if pkt.data.get("reply"):
            net.send(node, Packet("uni", "UNI", node, pkt.src, f"UNI {node}→{pkt.src}", 100,
                                  {"msg": f"chào {pkt.src}"}))

    net.on_deliver = deliver
    net.at(0, lambda: net.send("H1", Packet("uni", "UNI", "H1", "H2", "UNI H1→H2", 100,
                                            {"msg": "xin chào H2"})))
    net.at(5 * MS, lambda: net.send("H1", Packet("uni", "UNI", "H1", "H5", "UNI H1→H5", 100,
                                                 {"msg": "xin chào H5", "reply": True})))

    def fail():
        net.log("Liên kết R1–R2 hỏng → các router tính lại đường", "R1")
        net.set_link("R1", "R2", False)
        show_path(net, "H1", "H5")
        net.send("H1", Packet("uni", "UNI", "H1", "H5", "UNI H1→H5", 100, {"msg": "lần 2"}))
    net.at(50 * MS, fail)
    net.run()
    print()
    print("Tóm tắt unicast: mỗi gói chỉ tới đúng một đích; switch chỉ")
    print("  chuyển ra đúng một cổng; router giảm TTL, chọn chặng kế theo")
    print("  chi phí nhỏ nhất; đường đổi khi liên kết hỏng.")


# ---------------- Broadcast ----------------
def broadcast(net):
    def deliver(node, pkt):
        net.log(f"{node} nhận quảng bá \"{pkt.data['msg']}\" từ {pkt.src}", node)

    net.on_deliver = deliver
    net.at(0, lambda: net.send("H1", Packet("bc", "BC", "H1", "192.168.1.255", "BC 192.168.1.255",
                                            60, {"msg": "Ai có 192.168.1.13?"})))
    net.at(10 * MS, lambda: net.send("H4", Packet("bc", "BC", "H4", "255.255.255.255",
                                                  "BC 255.255.255.255", 60, {"msg": "DHCP DISCOVER"})))
    net.run()
    print()
    print("Tóm tắt broadcast: switch gửi tràn mọi cổng trong LAN; router")
    print("  chặn nên H4, H5, H6 không nhận quảng bá của H1 và LAN A không")
    print("  nhận quảng bá của H4 (miền quảng bá = một LAN).")


# ---------------- Multicast ----------------
G = "239.1.1.1"


def multicast(net):
    def deliver(node, pkt):
        net.log(f"{node} nhận {pkt.data['msg']} của nhóm {G}", node)

    def join(h, yes):
        def go():
            if yes:
                net.member[h].append(G)
            else:
                net.member[h].remove(G)
            what = "tham gia" if yes else "rời"
            net.log(f"{h} {what} nhóm {G}: gửi IGMP {'Report' if yes else 'Leave'}", h)
            net.send(h, Packet("igmp", "IGMP", h, G, f"IGMP {'Report' if yes else 'Leave'} {G}",
                               60, {"join": yes}))
        return go

    def send(n):
        def go():
            net.log(f"H1 gửi luồng #{n} tới nhóm {G}", "H1")
            net.send("H1", Packet("mc", "MC", "H1", G, f"MC {G} #{n}", 500, {"msg": f"luồng #{n}"}))
        return go

    net.on_deliver = deliver
    net.at(0, join("H2", True))
    net.at(5 * MS, send(1))
    net.at(20 * MS, join("H5", True))
    net.at(40 * MS, send(2))
    net.at(70 * MS, join("H6", True))
    net.at(75 * MS, join("H2", False))
    net.at(90 * MS, send(3))
    net.run()
    print()
    print("Tóm tắt multicast: chỉ thành viên nhóm nhận dữ liệu; switch")
    print("  (IGMP snooping) chỉ chuyển ra cổng có thành viên; router chỉ")
    print("  chuyển sang LAN có thành viên; mỗi liên kết chỉ mang một bản")
    print("  sao dù phía sau có nhiều thành viên.")


# ---------------- Anycast ----------------
A = "10.53.0.53"


def anycast(net):
    net.anycast[A] = ["H3", "H6"]
    print(f"Địa chỉ anycast {A} (DNS) do H3 và H6 cùng quảng bá.")
    for c in ["H1", "H5"]:
        costs = ", ".join(f"{s} chi phí {net.dist[c][s]}" for s in net.anycast[A])
        print(f"  {c}: {costs} → chọn {net.target(c, Packet('any', 'ANY', c, A, ''))}")

    def deliver(node, pkt):
        if pkt.cast == "any":
            net.log(f"{node} (phục vụ {A}) nhận \"{pkt.data['msg']}\" từ {pkt.src}", node)
            net.send(node, Packet("uni", "UNI", node, pkt.src, f"UNI {node}→{pkt.src}", 120,
                                  {"msg": f"trả lời từ {node}"}))
        else:
            net.log(f"{node} nhận \"{pkt.data['msg']}\"", node)

    def query(c, n):
        def go():
            net.log(f"{c} gửi truy vấn #{n} tới {A}", c)
            net.send(c, Packet("any", "ANY", c, A, f"ANY {c}→{A}", 80, {"msg": f"truy vấn #{n}"}))
        return go

    def down():
        net.log(f"H3 ngừng hoạt động → rút quảng bá {A}", "H3")
        net.set_node("H3", False)
        best = net.target("H1", Packet("any", "ANY", "H1", A, ""))
        print(f"  H1: còn H6 chi phí {net.dist['H1']['H6']} → chọn {best}")

    net.on_deliver = deliver
    net.at(0, query("H1", 1))
    net.at(2 * MS, query("H5", 1))
    net.at(40 * MS, down)
    net.at(41 * MS, query("H1", 2))
    net.run()
    print()
    print("Tóm tắt anycast: cùng một địa chỉ, gói tới máy phục vụ gần nhất")
    print("  theo chi phí định tuyến; khi H3 hỏng, truy vấn của H1 được")
    print("  chuyển sang H6 (dự phòng tự động).")


__all__ = ["udp", "unicast", "broadcast", "multicast", "anycast", "fmt_t"]
