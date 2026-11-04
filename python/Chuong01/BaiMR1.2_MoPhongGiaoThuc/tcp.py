"""Bài mở rộng MR1.2 – Kịch bản TCP: bắt tay ba bước, cửa sổ trượt, điều khiển tắc nghẽn, đóng FIN.

TCP được rút gọn để dễ theo dõi (dựa trên TCP Reno, RFC 9293 và RFC 5681):
- bắt tay SYN → SYN+ACK → ACK; đóng FIN → ACK, FIN → ACK;
- bên nhận xác nhận tích lũy (cumulative ACK) từng đoạn, giữ lại đoạn đến sớm (ngoài thứ tự);
- bên gửi chỉ gửi khi số byte đang bay < cwnd (cửa sổ tắc nghẽn);
- khởi đầu chậm (slow start): cwnd < ssthresh thì mỗi ACK mới tăng cwnd thêm 1 MSS (gấp đôi mỗi RTT);
- tránh tắc nghẽn (congestion avoidance): mỗi ACK mới tăng MSS·MSS/cwnd (≈ +1 MSS mỗi RTT);
- 3 ACK trùng → gửi lại nhanh (fast retransmit): ssthresh = max(đang bay/2, 2 MSS), cwnd = ssthresh;
- hết giờ (RTO) → ssthresh = max(đang bay/2, 2 MSS), cwnd = 1 MSS, gửi lại từ đoạn chưa xác nhận,
  RTO tăng gấp đôi (back-off).
Đơn giản hóa: RTO cố định 100 ms ban đầu (RFC 6298 tính từ RTT đo được), không thổi phồng cwnd
trong khôi phục nhanh, không có ACK trễ, cửa sổ bên nhận đủ lớn.
"""
from mang import Packet, fmt_t

MSS = 1000
HDR = 40           # byte tiêu đề TCP/IP
W = 34             # độ rộng phần mũi tên trong sơ đồ


def arrow(label, right, lost):
    """Mũi tên giữa hai trục thời gian: ─ nhãn ───► hoặc ◄─── nhãn ─, mất gói thì dừng giữa đường bằng ×."""
    if right:
        s = "─ " + label + " "
        s = s + "─" * (W - 1 - len(s)) + "►" if not lost else (s + "─" * 4 + "×").ljust(W)
    else:
        s = " " + label + " ─"
        s = "◄" + "─" * (W - 1 - len(s)) + s if not lost else ("×" + "─" * 4 + s).rjust(W)
    return s


def cw(c):
    return f"{c // MSS}.{c % MSS * 10 // MSS}"


class TcpScenario:
    def __init__(self, net, nseg, drops, client="H1", server="H5"):
        self.net, self.c, self.s = net, client, server
        self.total = nseg * MSS
        self.drops = list(drops)             # các đoạn bị ép mất (lặp số đoạn = mất nhiều lần)
        self.cwnd, self.ssthresh = MSS, 8 * MSS
        self.una = self.nxt = 1              # byte đầu tiên chưa được xác nhận / sẽ gửi tiếp
        self.dup, self.recovering = 0, False
        self.rto, self.gen, self.timer_on = 100_000, 0, False
        self.sent = {}                       # đoạn -> số lần đã gửi
        self.tx = self.retx = self.fast = self.timeouts = 0
        self.rnext, self.rbuf = 1, set()     # bên nhận: byte mong đợi tiếp theo, đoạn đến sớm
        self.rows = []                       # dòng sơ đồ: (thời điểm, thứ tự, văn bản)
        self.cwnd_log = [(0, MSS)]
        self.events = []                     # (thời điểm, 'F' hoặc 'T') cho biểu đồ
        self.state = "CLOSED"
        self.t_data0 = self.t_done = 0
        net.hop_trace = False
        net.on_deliver = self.deliver
        net.on_loss = self.loss

    # ---------- ghi sơ đồ ----------
    def row(self, t, text):
        self.rows.append((t, len(self.rows), text))

    def pkt_row(self, pkt, t_recv, note, lost=False):
        right = pkt.src == self.c
        recv = "×" if lost else fmt_t(t_recv)
        body = arrow(pkt.label, right, lost)
        self.row(pkt.data["t"], f"{fmt_t(pkt.data['t']):>6} {recv:>6}  |{body}|  {note}")
        to = self.s if right else self.c                 # ghi chú cho hoạt hình HTML
        what = f"× {pkt.label} {note}" if lost else f"{to} nhận {pkt.label} {note}"
        self.net.notes.append([self.net.now, to, what.rstrip()])

    def event(self, text):
        self.row(self.net.now, f"{fmt_t(self.net.now):>6} {'':>6}  => {text}")
        self.net.notes.append([self.net.now, self.c, text])

    def set_cwnd(self, v):
        self.cwnd = v
        self.cwnd_log.append((self.net.now, v))

    # ---------- gửi ----------
    def out(self, frm, to, label, flags, seq=0, ack=0, length=0, drop=False):
        p = Packet("uni", "TCP", frm, to, label, HDR + length,
                   {"flags": flags, "seq": seq, "ack": ack, "len": length, "t": self.net.now})
        if drop:
            p.drop_on = ("R2", "R3") if frm == self.c else ("R2", "R1")
        self.net.send(frm, p)

    def start(self):
        self.state = "SYN_SENT"
        self.out(self.c, self.s, "SYN seq=0", "SYN")

    def send_data(self):
        while self.nxt <= self.total and self.nxt - self.una + MSS <= self.cwnd:
            k = (self.nxt - 1) // MSS + 1
            n = self.sent.get(k, 0) + 1
            self.sent[k] = n
            self.tx += 1
            again = n > 1
            if again:
                self.retx += 1
            label = f"#{k} seq={self.nxt}" + (" gửi lại" if again else "")
            self.out(self.c, self.s, label, "DATA", seq=self.nxt, length=MSS,
                     drop=self.drops.count(k) >= n)
            self.nxt += MSS
            if not self.timer_on:
                self.arm()

    def arm(self):
        self.gen += 1
        self.timer_on = True
        g = self.gen
        self.net.at(self.net.now + self.rto, lambda: self.timeout(g))

    def stop_timer(self):
        self.gen += 1
        self.timer_on = False

    def timeout(self, g):
        if g != self.gen:
            return                           # bộ hẹn giờ này đã bị hủy/khởi động lại
        self.timeouts += 1
        flight = self.nxt - self.una
        self.ssthresh = max(flight // 2, 2 * MSS)
        self.set_cwnd(MSS)
        self.events.append((self.net.now, "T"))
        self.event(f"HẾT GIỜ RTO={self.rto // 1000} ms: ssthresh={cw(self.ssthresh)}, cwnd=1")
        self.rto = min(self.rto * 2, 800_000)
        self.nxt, self.dup, self.recovering = self.una, 0, False
        self.timer_on = False
        self.send_data()

    # ---------- nhận ----------
    def deliver(self, node, pkt):
        d = self.net
        f = pkt.data["flags"]
        if node == self.s:                                   # ---- phía máy chủ H5
            if f == "SYN":
                self.pkt_row(pkt, d.now, "")
                self.out(self.s, self.c, "SYN+ACK seq=0 ack=1", "SYNACK", ack=1)
            elif f == "ACK" and self.state == "SYN_SENT2":
                self.pkt_row(pkt, d.now, "ESTABLISHED")
                self.state = "ESTABLISHED"
            elif f == "DATA":
                seq = pkt.data["seq"]
                if seq == self.rnext:
                    self.rnext += MSS
                    while self.rnext in self.rbuf:          # đoạn đến sớm nay đã liền mạch
                        self.rbuf.discard(self.rnext)
                        self.rnext += MSS
                elif seq > self.rnext:
                    self.rbuf.add(seq)
                self.pkt_row(pkt, d.now, "")
                self.out(self.s, self.c, f"ACK {self.rnext}", "ACK", ack=self.rnext)
            elif f == "FIN":
                self.pkt_row(pkt, d.now, "CLOSE_WAIT")
                self.out(self.s, self.c, f"ACK {pkt.data['seq'] + 1}", "FINACK", ack=pkt.data["seq"] + 1)
                d.at(d.now + 1000, lambda: self.out(self.s, self.c, "FIN seq=1", "FIN2", seq=1))
            elif f == "LASTACK":
                self.pkt_row(pkt, d.now, "CLOSED")
            return
        # ---- phía khách H1
        if f == "SYNACK":
            self.pkt_row(pkt, d.now, "")
            self.state = "SYN_SENT2"
            self.out(self.c, self.s, "ACK ack=1", "ACK", ack=1)
            self.t_data0 = d.now
            self.send_data()
        elif f == "ACK":
            self.on_ack(pkt)
        elif f == "FINACK":
            self.pkt_row(pkt, d.now, "FIN_WAIT_2")
        elif f == "FIN2":
            self.pkt_row(pkt, d.now, "TIME_WAIT")
            self.out(self.c, self.s, "ACK ack=2", "LASTACK", ack=2)

    def on_ack(self, pkt):
        ack = pkt.data["ack"]
        if ack > self.una:                                   # ACK mới
            self.una = ack
            self.nxt = max(self.nxt, ack)
            self.dup = 0
            if self.recovering:
                self.recovering = False
                self.set_cwnd(self.ssthresh)
            elif self.cwnd < self.ssthresh:
                self.set_cwnd(self.cwnd + MSS)                # khởi đầu chậm
            else:
                self.set_cwnd(self.cwnd + MSS * MSS // self.cwnd)   # tránh tắc nghẽn
            self.rto = 100_000
            self.pkt_row(pkt, self.net.now, f"cwnd={cw(self.cwnd)}")
            if self.una > self.total:
                self.stop_timer()
                if self.state == "ESTABLISHED" or self.state == "SYN_SENT2":
                    self.state = "FIN_WAIT_1"
                    self.t_done = self.net.now
                    self.out(self.c, self.s, f"FIN seq={self.total + 1}", "FIN", seq=self.total + 1)
                return
            self.arm()
            self.send_data()
        elif ack == self.una:                                # ACK trùng
            self.dup += 1
            self.pkt_row(pkt, self.net.now, f"trùng {self.dup}")
            if self.dup == 3:
                self.fast += 1
                self.ssthresh = max((self.nxt - self.una) // 2, 2 * MSS)
                self.set_cwnd(self.ssthresh)
                self.recovering = True
                self.events.append((self.net.now, "F"))
                self.event(f"3 ACK TRÙNG: gửi lại nhanh, ssthresh=cwnd={cw(self.cwnd)}")
                k = (self.una - 1) // MSS + 1
                self.sent[k] += 1
                self.tx += 1
                self.retx += 1
                self.out(self.c, self.s, f"#{k} seq={self.una} gửi lại", "DATA", seq=self.una,
                         length=MSS, drop=self.drops.count(k) >= self.sent[k])

    def loss(self, pkt, a, b):
        self.pkt_row(pkt, 0, f"MẤT {a}→{b}", lost=True)

    # ---------- kết quả ----------
    def report(self):
        print(f"Sơ đồ thời gian – trình tự ({self.c} khách, {self.s} máy chủ; thời gian ms):")
        print(f"{'gửi':>6} {'nhận':>6}  {self.c:<{W + 1}}{self.s}")
        for _, _, text in sorted(self.rows):
            print(text.rstrip())
        print()
        self.chart()
        dur = self.t_done - self.t_data0
        print()
        print(f"Tóm tắt TCP: {self.total // MSS} đoạn ({self.total} byte), truyền {self.tx} lần,"
              f" gửi lại {self.retx}")
        print(f"  ({self.fast} lần gửi lại nhanh, {self.timeouts} lần hết giờ);"
              f" xong sau {fmt_t(dur)} ms,")
        print(f"  thông lượng ≈ {self.total * 8 * 1000 // dur} kbit/s"
              f" (liên kết chậm nhất 10 Mbit/s,")
        print("  RTT ≈ 26 ms): cửa sổ nhỏ và mất gói làm thông lượng thấp hơn")
        print("  nhiều so với băng thông.")

    def chart(self):
        cols, t_end = 52, self.cwnd_log[-1][0]
        vals = []
        for c in range(cols):
            t = t_end * (c + 1) // cols
            v = MSS
            for tt, cv in self.cwnd_log:
                if tt <= t:
                    v = cv
            vals.append(v // MSS)
        top = max(vals)
        print(f"Biểu đồ cwnd (đơn vị MSS = {MSS} byte) theo thời gian:")
        for y in range(top, 0, -1):
            line = "".join("█" if v >= y else " " for v in vals)
            print(f"{y:>4} │{line}")
        print("     └" + "─" * cols)
        marks = [" "] * cols
        for t, m in self.events:
            marks[min(cols - 1, t * cols // max(t_end, 1))] = m
        print("      " + "".join(marks).rstrip())
        right = f"{fmt_t(t_end)} ms"
        print("      0" + right.rjust(cols - 1))
        print("      F = gửi lại nhanh (3 ACK trùng), T = hết giờ RTO")
