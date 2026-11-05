import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Kịch bản TCP: bắt tay ba bước, cửa sổ trượt, điều khiển tắc nghẽn, đóng FIN.
 *
 * TCP được rút gọn để dễ theo dõi (dựa trên TCP Reno, RFC 9293 và RFC 5681):
 *  - bắt tay SYN → SYN+ACK → ACK; đóng FIN → ACK, FIN → ACK;
 *  - bên nhận xác nhận tích lũy từng đoạn, giữ lại đoạn đến sớm (ngoài thứ tự);
 *  - bên gửi chỉ gửi khi số byte đang bay < cwnd (cửa sổ tắc nghẽn);
 *  - khởi đầu chậm: cwnd < ssthresh thì mỗi ACK mới tăng cwnd thêm 1 MSS (gấp đôi mỗi RTT);
 *  - tránh tắc nghẽn: mỗi ACK mới tăng MSS·MSS/cwnd (≈ +1 MSS mỗi RTT);
 *  - 3 ACK trùng → gửi lại nhanh: ssthresh = max(đang bay/2, 2 MSS), cwnd = ssthresh;
 *  - hết giờ (RTO) → ssthresh = max(đang bay/2, 2 MSS), cwnd = 1 MSS, gửi lại từ đoạn chưa
 *    xác nhận, RTO tăng gấp đôi.
 * Đơn giản hóa: RTO cố định 100 ms ban đầu (RFC 6298 tính từ RTT đo được), không thổi phồng cwnd
 * trong khôi phục nhanh, không có ACK trễ, cửa sổ bên nhận đủ lớn.
 */
final class TcpKichBan {
    static final int MSS = 1000;
    static final int HDR = 40;                             // byte tiêu đề TCP/IP
    static final int W = 34;                               // độ rộng phần mũi tên trong sơ đồ

    /** Mũi tên giữa hai trục thời gian; mất gói thì dừng giữa đường bằng ×. */
    static String arrow(String label, boolean right, boolean lost) {
        if (right) {
            String s = "─ " + label + " ";
            if (!lost) return s + "─".repeat(W - 1 - s.length()) + "►";
            s += "─".repeat(4) + "×";
            return s + " ".repeat(Math.max(0, W - s.length()));
        }
        String s = " " + label + " ─";
        if (!lost) return "◄" + "─".repeat(W - 1 - s.length()) + s;
        s = "×" + "─".repeat(4) + s;
        return " ".repeat(Math.max(0, W - s.length())) + s;
    }

    static String cw(int c) { return (c / MSS) + "." + (c % MSS * 10 / MSS); }

    record Row(long t, String text) {}

    final Mang net;
    final String c = "H1", s = "H5";
    final int total;
    final List<Integer> drops;                             // các đoạn bị ép mất (lặp số đoạn = mất nhiều lần)
    int cwnd = MSS, ssthresh = 8 * MSS;
    int una = 1, nxt = 1;                                  // byte đầu tiên chưa được xác nhận / sẽ gửi tiếp
    int dup;
    boolean recovering;
    long rto = 100_000;
    int gen;
    boolean timerOn;
    final Map<Integer, Integer> sent = new HashMap<>();    // đoạn -> số lần đã gửi
    int tx, retx, fast, timeouts;
    int rnext = 1;                                         // bên nhận: byte mong đợi tiếp theo
    final Set<Integer> rbuf = new HashSet<>();             // bên nhận: đoạn đến sớm
    final List<Row> rows = new ArrayList<>();
    final List<long[]> cwndLog = new ArrayList<>();
    final List<Object[]> events = new ArrayList<>();       // {thời điểm, "F" | "T"}
    String state = "CLOSED";
    long tData0, tDone;

    TcpKichBan(Mang net, int nseg, List<Integer> drops) {
        this.net = net;
        this.total = nseg * MSS;
        this.drops = drops;
        cwndLog.add(new long[] {0, MSS});
        net.hopTrace = false;
        net.onDeliver = this::deliver;
        net.onLoss = this::loss;
    }

    // ---------- ghi sơ đồ ----------
    void pktRow(Mang.Packet p, long tRecv, String note, boolean lost) {
        boolean right = p.src.equals(c);
        String recv = lost ? "×" : Mang.fmtT(tRecv);
        rows.add(new Row(p.t, String.format("%6s %6s  |%s|  %s", Mang.fmtT(p.t), recv, arrow(p.label, right, lost), note)));
        String to = right ? s : c;                         // ghi chú cho hoạt hình HTML
        String what = lost ? "× " + p.label + " " + note : to + " nhận " + p.label + " " + note;
        net.notes.add(List.of(net.now, to, what.stripTrailing()));
    }

    void event(String text) {
        rows.add(new Row(net.now, String.format("%6s %6s  => %s", Mang.fmtT(net.now), "", text)));
        net.notes.add(List.of(net.now, c, text));
    }

    void setCwnd(int v) {
        cwnd = v;
        cwndLog.add(new long[] {net.now, v});
    }

    // ---------- gửi ----------
    void out(String frm, String to, String label, String flags, int seq, int ack, int length, boolean drop) {
        Mang.Packet p = new Mang.Packet("uni", "TCP", frm, to, label, HDR + length);
        p.flags = flags; p.seq = seq; p.ack = ack; p.t = net.now;
        if (drop) {
            p.dropA = "R2";
            p.dropB = frm.equals(c) ? "R3" : "R1";
        }
        net.send(frm, p);
    }

    void start() {
        state = "SYN_SENT";
        out(c, s, "SYN seq=0", "SYN", 0, 0, 0, false);
    }

    void sendData() {
        while (nxt <= total && nxt - una + MSS <= cwnd) {
            int k = (nxt - 1) / MSS + 1;
            int n = sent.getOrDefault(k, 0) + 1;
            sent.put(k, n);
            tx++;
            String label = "#" + k + " seq=" + nxt;
            if (n > 1) {
                retx++;
                label += " gửi lại";
            }
            out(c, s, label, "DATA", nxt, 0, MSS, Collections.frequency(drops, k) >= n);
            nxt += MSS;
            if (!timerOn) arm();
        }
    }

    void arm() {
        gen++;
        timerOn = true;
        int g = gen;
        net.at(net.now + rto, () -> timeout(g));
    }

    void stopTimer() {
        gen++;
        timerOn = false;
    }

    void timeout(int g) {
        if (g != gen) return;                              // bộ hẹn giờ này đã bị hủy/khởi động lại
        timeouts++;
        ssthresh = Math.max((nxt - una) / 2, 2 * MSS);
        setCwnd(MSS);
        events.add(new Object[] {net.now, "T"});
        event("HẾT GIỜ RTO=" + (rto / 1000) + " ms: ssthresh=" + cw(ssthresh) + ", cwnd=1");
        rto = Math.min(rto * 2, 800_000);
        nxt = una; dup = 0; recovering = false;
        timerOn = false;
        sendData();
    }

    // ---------- nhận ----------
    void deliver(String node, Mang.Packet p) {
        String f = p.flags;
        if (node.equals(s)) {                              // ---- phía máy chủ H5
            if (f.equals("SYN")) {
                pktRow(p, net.now, "", false);
                out(s, c, "SYN+ACK seq=0 ack=1", "SYNACK", 0, 1, 0, false);
            } else if (f.equals("ACK") && state.equals("SYN_SENT2")) {
                pktRow(p, net.now, "ESTABLISHED", false);
                state = "ESTABLISHED";
            } else if (f.equals("DATA")) {
                if (p.seq == rnext) {
                    rnext += MSS;
                    while (rbuf.remove(rnext)) rnext += MSS;   // đoạn đến sớm nay đã liền mạch
                } else if (p.seq > rnext) {
                    rbuf.add(p.seq);
                }
                pktRow(p, net.now, "", false);
                out(s, c, "ACK " + rnext, "ACK", 0, rnext, 0, false);
            } else if (f.equals("FIN")) {
                pktRow(p, net.now, "CLOSE_WAIT", false);
                out(s, c, "ACK " + (p.seq + 1), "FINACK", 0, p.seq + 1, 0, false);
                net.at(net.now + 1000, () -> out(s, c, "FIN seq=1", "FIN2", 1, 0, 0, false));
            } else if (f.equals("LASTACK")) {
                pktRow(p, net.now, "CLOSED", false);
            }
            return;
        }
        switch (f) {                                       // ---- phía khách H1
            case "SYNACK" -> {
                pktRow(p, net.now, "", false);
                state = "SYN_SENT2";
                out(c, s, "ACK ack=1", "ACK", 0, 1, 0, false);
                tData0 = net.now;
                sendData();
            }
            case "ACK" -> onAck(p);
            case "FINACK" -> pktRow(p, net.now, "FIN_WAIT_2", false);
            case "FIN2" -> {
                pktRow(p, net.now, "TIME_WAIT", false);
                out(c, s, "ACK ack=2", "LASTACK", 0, 2, 0, false);
            }
            default -> { }
        }
    }

    void onAck(Mang.Packet p) {
        int ack = p.ack;
        if (ack > una) {                                   // ACK mới
            una = ack;
            nxt = Math.max(nxt, ack);
            dup = 0;
            if (recovering) {
                recovering = false;
                setCwnd(ssthresh);
            } else if (cwnd < ssthresh) {
                setCwnd(cwnd + MSS);                       // khởi đầu chậm
            } else {
                setCwnd(cwnd + MSS * MSS / cwnd);          // tránh tắc nghẽn
            }
            rto = 100_000;
            pktRow(p, net.now, "cwnd=" + cw(cwnd), false);
            if (una > total) {
                stopTimer();
                if (state.equals("ESTABLISHED") || state.equals("SYN_SENT2")) {
                    state = "FIN_WAIT_1";
                    tDone = net.now;
                    out(c, s, "FIN seq=" + (total + 1), "FIN", total + 1, 0, 0, false);
                }
                return;
            }
            arm();
            sendData();
        } else if (ack == una) {                           // ACK trùng
            dup++;
            pktRow(p, net.now, "trùng " + dup, false);
            if (dup == 3) {
                fast++;
                ssthresh = Math.max((nxt - una) / 2, 2 * MSS);
                setCwnd(ssthresh);
                recovering = true;
                events.add(new Object[] {net.now, "F"});
                event("3 ACK TRÙNG: gửi lại nhanh, ssthresh=cwnd=" + cw(cwnd));
                int k = (una - 1) / MSS + 1;
                sent.merge(k, 1, Integer::sum);
                tx++;
                retx++;
                out(c, s, "#" + k + " seq=" + una + " gửi lại", "DATA", una, 0, MSS,
                        Collections.frequency(drops, k) >= sent.get(k));
            }
        }
    }

    void loss(Mang.Packet p, String a, String b) {
        pktRow(p, 0, "MẤT " + a + "→" + b, true);
    }

    // ---------- kết quả ----------
    void report() {
        System.out.printf("Sơ đồ thời gian – trình tự (%s khách, %s máy chủ; thời gian ms):%n", c, s);
        System.out.printf("%6s %6s  %-" + (W + 1) + "s%s%n", "gửi", "nhận", c, s);
        rows.sort((x, y) -> Long.compare(x.t(), y.t()));   // List.sort ổn định: cùng thời điểm giữ thứ tự ghi
        for (Row r : rows) System.out.println(r.text().stripTrailing());
        System.out.println();
        chart();
        long dur = tDone - tData0;
        System.out.println();
        System.out.printf("Tóm tắt TCP: %d đoạn (%d byte), truyền %d lần, gửi lại %d%n", total / MSS, total, tx, retx);
        System.out.printf("  (%d lần gửi lại nhanh, %d lần hết giờ); xong sau %s ms,%n", fast, timeouts, Mang.fmtT(dur));
        System.out.printf("  thông lượng ≈ %d kbit/s (liên kết chậm nhất 10 Mbit/s,%n", (long) total * 8 * 1000 / dur);
        System.out.println("  RTT ≈ 26 ms): cửa sổ nhỏ và mất gói làm thông lượng thấp hơn");
        System.out.println("  nhiều so với băng thông.");
    }

    void chart() {
        final int cols = 52;
        long tEnd = cwndLog.get(cwndLog.size() - 1)[0];
        int[] vals = new int[cols];
        int top = 0;
        for (int k = 0; k < cols; k++) {
            long t = tEnd * (k + 1) / cols;
            long v = MSS;
            for (long[] pt : cwndLog) if (pt[0] <= t) v = pt[1];
            vals[k] = (int) (v / MSS);
            top = Math.max(top, vals[k]);
        }
        System.out.printf("Biểu đồ cwnd (đơn vị MSS = %d byte) theo thời gian:%n", MSS);
        for (int y = top; y > 0; y--) {
            StringBuilder b = new StringBuilder();
            for (int v : vals) b.append(v >= y ? "█" : " ");
            System.out.printf("%4d │%s%n", y, b);
        }
        System.out.println("     └" + "─".repeat(cols));
        char[] marks = " ".repeat(cols).toCharArray();
        for (Object[] e : events)
            marks[(int) Math.min(cols - 1, (Long) e[0] * cols / Math.max(tEnd, 1))] = ((String) e[1]).charAt(0);
        System.out.println("      " + new String(marks).stripTrailing());
        String right = Mang.fmtT(tEnd) + " ms";
        System.out.printf("      0%" + (cols - 1) + "s%n", right);
        System.out.println("      F = gửi lại nhanh (3 ACK trùng), T = hết giờ RTO");
    }
}
