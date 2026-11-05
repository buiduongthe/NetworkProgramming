import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.nio.ByteBuffer;
import java.nio.channels.SelectionKey;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.TreeSet;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài mở rộng MR5.5 – Server bất đồng bộ trên vòng lặp Selector một luồng, phối hợp với nhóm luồng và luồng nền.
 * Các luồng của tiến trình server:
 *   vòng-lặp    – luồng chạy VongLap: accept, đọc/ghi socket không chặn, nhịp tim 10 ms;
 *   tính-1, -2  – nhóm luồng RIÊNG (newFixedThreadPool) cho việc băm: CompletableFuture.supplyAsync(việc, nhómTính);
 *   ảo#k        – luồng ảo (Java 21) cho hàm thư viện cũ kiểu chặn: newVirtualThreadPerTaskExecutor();
 *   cảm-biến    – một luồng nền chạy vòng lặp chặn của riêng nó; đẩy mẫu vào vòng lặp bằng vongLap.execute(...)
 *                 và hoàn tất CompletableFuture (complete() từ luồng khác) khi vòng lặp nhờ đọc ngay.
 * Hủy, hạn chót: future.orTimeout(hạn) hoàn tất ngoại lệ TimeoutException → vòng lặp bật cờ AtomicBoolean mà
 * luồng băm kiểm tra mỗi vòng. (future.cancel(true) KHÔNG ngắt được việc đang chạy của CompletableFuture.)
 */
final class MayChu {
    final VongLap loop;
    final ExecutorService nhomTinh;
    final ExecutorService luongAo = Executors.newVirtualThreadPerTaskExecutor();
    final LinkedBlockingQueue<CompletableFuture<Object[]>> ycCamBien = new LinkedBlockingQueue<>();

    // Thống kê – chỉ luồng vòng lặp đọc/ghi
    double treMau;
    int soMau, oNhom, oNhomMax, soYC, dangVao;
    final ArrayDeque<Runnable> choVao = new ArrayDeque<>();      // chế độ C: yêu cầu chờ ở cổng vào
    final Map<String, TreeSet<String>> luong = new LinkedHashMap<>();

    MayChu(VongLap loop) {
        this.loop = loop;
        AtomicInteger dem = new AtomicInteger();
        nhomTinh = Executors.newFixedThreadPool(Chung.NHOM_TINH, r -> {
            Thread t = new Thread(r, "tính-" + dem.incrementAndGet());
            t.setDaemon(true);
            return t;
        });
        datLai();
    }

    void datLai() {
        loop.treNhipMax = 0;
        treMau = 0;
        soMau = oNhomMax = soYC = 0;
        for (String gd : new String[] {"nhận/trả lời", "tra cứu cũ", "băm SHA-256"}) luong.put(gd, new TreeSet<>());
    }

    /** Trạng thái đọc của một kết nối (gắn vào SelectionKey). */
    static final class KetNoi {
        final ByteArrayOutputStream buf = new ByteArrayOutputStream();
        final SocketChannel ch;
        KetNoi(SocketChannel ch) { this.ch = ch; }
    }

    // ---------- luồng cảm biến ----------
    void camBien() {
        int k = 0;
        while (true) {
            try {
                CompletableFuture<Object[]> cf = ycCamBien.poll(Chung.CAM_BIEN_MS, TimeUnit.MILLISECONDS);
                k++;
                if (cf != null) cf.complete(new Object[] {(k * 37) % 100, Chung.tenLuong()});   // từ luồng khác: an toàn
                else {
                    int kk = k;
                    double tTao = Chung.bayGioMs();
                    loop.execute(() -> nhanMau(kk, tTao));       // đưa việc vào vòng lặp
                }
            } catch (InterruptedException e) { return; }
        }
    }

    void nhanMau(int k, double tTao) {                          // chạy trên vòng lặp: không cần khóa
        soMau++;
        treMau = Math.max(treMau, Chung.bayGioMs() - tTao);
    }

    // ---------- trả lời ----------
    static void traLoi(SocketChannel ch, String s) {
        try (ch) {
            ByteBuffer bb = ByteBuffer.wrap((s + "\n").getBytes(StandardCharsets.UTF_8));
            while (bb.hasRemaining()) ch.write(bb);   // dòng ngắn: bộ đệm gửi của HĐH luôn đủ chỗ trong bài này
        } catch (IOException e) { Chung.log("lỗi ghi: " + e.getMessage()); }
    }

    // ---------- xử lý BAM ----------
    void xuLyBam(SocketChannel ch, String[] p, byte[] data) {
        String ma = p[1], cheDo = p[2];
        int vong = Integer.parseInt(p[4]), han = Integer.parseInt(p[5]);
        boolean ghi = ma.equals("1") || ma.equals("101") || ma.equals("102");
        if (ghi) Chung.log(Chung.f("BAM %s (%s): nhận %d byte, %d vòng, hạn %d ms", ma, cheDo, data.length, vong, han));
        soYC++;
        if (cheDo.equals("chan")) {                             // A. SAI: chặn vòng lặp sự kiện
            luong.get("tra cứu cũ").add(Chung.traCuuCu());
            Chung.KetQuaBam kq = Chung.bam(data, vong, null);
            luong.get("băm SHA-256").add(kq.ten());
            if (ghi) Chung.log("BAM " + ma + ": băm xong trên " + kq.ten() + ", trả lời OK");
            traLoi(ch, "OK " + ma + " " + kq.hex());
            return;
        }
        Runnable batDau = () -> tach(ch, ma, data, vong, han, ghi, cheDo.equals("gioihan"));
        if (cheDo.equals("gioihan") && dangVao >= Chung.GIOI_HAN) choVao.add(batDau);   // C. chờ ở cổng vào
        else batDau.run();
    }

    void tach(SocketChannel ch, String ma, byte[] data, int vong, int han, boolean ghi, boolean gioiHan) {
        if (gioiHan) dangVao++;
        AtomicBoolean huy = new AtomicBoolean();                // cờ hủy chia sẻ với luồng băm
        CompletableFuture<Chung.KetQuaBam> ketQua = new CompletableFuture<>();
        CompletableFuture.supplyAsync(Chung::traCuuCu, luongAo)   // hàm chặn → luồng ảo
            .thenAcceptAsync(tenCu -> {                           // ...rồi quay về vòng lặp
                luong.get("tra cứu cũ").add(tenCu);
                if (huy.get()) return;                            // đã quá hạn khi đang tra cứu → không băm nữa
                oNhom++;
                oNhomMax = Math.max(oNhomMax, oNhom);
                CompletableFuture.supplyAsync(() -> Chung.bam(data, vong, huy), nhomTinh)   // CPU → nhóm riêng
                    .thenAcceptAsync(kq -> {                      // luồng băm xong THẬT → về vòng lặp
                        oNhom--;
                        if (kq.hex() == null) Chung.say(Chung.f("[server] BAM %s: luồng tính thấy cờ hủy → dừng ở vòng %d", ma, kq.dungO()));
                        else ketQua.complete(kq);
                    }, loop);
            }, loop);
        ketQua.orTimeout(han, TimeUnit.MILLISECONDS)              // hạn chót
            .whenCompleteAsync((kq, loi) -> {
                if (gioiHan) {                                    // trả "suất" ở cổng vào, cho yêu cầu kế tiếp vào
                    dangVao--;
                    Runnable tiep = choVao.poll();
                    if (tiep != null) tiep.run();
                }
                if (loi != null) {
                    huy.set(true);                                // hủy đi xuyên ranh giới: báo luồng băm dừng
                    Chung.log(Chung.f("BAM %s: quá hạn %d ms → trả HETGIO, bật cờ hủy cho luồng tính", ma, han));
                    traLoi(ch, "HETGIO " + ma);
                    return;
                }
                luong.get("băm SHA-256").add(kq.ten());
                if (ghi) Chung.log("BAM " + ma + ": băm xong trên " + kq.ten() + ", trả lời OK");
                traLoi(ch, "OK " + ma + " " + kq.hex());
            }, loop);
    }

    // ---------- điều phối một yêu cầu đã đọc đủ ----------
    void xuLy(SocketChannel ch, String[] p, byte[] data) {
        switch (p[0]) {
            case "BAM" -> xuLyBam(ch, p, data);
            case "CAMBIEN" -> {
                CompletableFuture<Object[]> cf = new CompletableFuture<>();
                ycCamBien.add(cf);                                // nhờ luồng cảm biến đọc ngay
                cf.orTimeout(1, TimeUnit.SECONDS).thenAcceptAsync(v -> {
                    Chung.log(Chung.f("CAMBIEN %s: future được hoàn tất từ luồng %s, giá trị %d", p[1], v[1], v[0]));
                    traLoi(ch, Chung.f("CAMBIEN %s %d %s", p[1], v[0], v[1]));
                }, loop);
            }
            case "DATLAI" -> { datLai(); traLoi(ch, "OK"); }
            default -> {                                          // THONGKE nhãn
                String nhan = String.join(" ", Arrays.copyOfRange(p, 1, p.length));
                Chung.say(Chung.f("[server] đợt \"%s\": %d yêu cầu", nhan, soYC));
                Chung.say(Chung.f("  %-15sluồng đã chạy", "giai đoạn"));
                luong.forEach((gd, ds) -> Chung.say(Chung.f("  %-15s%s", gd, tomTat(ds))));
                Chung.say(Chung.f("  trễ nhịp tối đa %.1f ms; nhóm tính tối đa %d việc", loop.treNhipMax, oNhomMax));
                Chung.say(Chung.f("  cảm biến %d mẫu, trễ giao tối đa %.1f ms", soMau, treMau));
                traLoi(ch, Chung.f("TK %.1f %d %.1f %d", loop.treNhipMax, soMau, treMau, oNhomMax));
            }
        }
    }

    /** Danh sách tên luồng; quá 8 tên thì chỉ in 3 tên đầu và tổng số. */
    static String tomTat(TreeSet<String> ds) {
        if (ds.isEmpty()) return "–";
        if (ds.size() <= 8) return String.join(", ", ds);
        return String.join(", ", ds.stream().limit(3).toList()) + ", … (" + ds.size() + " luồng)";
    }

    // ---------- sự kiện socket từ Selector ----------
    void suKienSocket(SelectionKey key) {
        try {
            if (key.isAcceptable()) {
                SocketChannel ch = ((ServerSocketChannel) key.channel()).accept();
                if (ch == null) return;
                ch.configureBlocking(false);
                ch.register(loop.sel, SelectionKey.OP_READ, new KetNoi(ch));
                luong.get("nhận/trả lời").add(Chung.tenLuong());
                return;
            }
            KetNoi kn = (KetNoi) key.attachment();
            ByteBuffer bb = ByteBuffer.allocate(65536);
            int n = kn.ch.read(bb);
            if (n < 0) { key.cancel(); kn.ch.close(); return; }
            kn.buf.write(bb.array(), 0, n);
            byte[] all = kn.buf.toByteArray();
            int j = -1;
            for (int i = 0; i < all.length; i++) if (all[i] == '\n') { j = i; break; }
            if (j < 0) return;                                    // chưa đủ dòng lệnh
            String[] p = new String(all, 0, j, StandardCharsets.UTF_8).trim().split(" ");
            int can = p[0].equals("BAM") && p.length == 6 ? Integer.parseInt(p[3]) : 0;
            if (all.length - j - 1 < can) return;                 // chưa đủ dữ liệu
            key.interestOps(0);                                   // đã đủ yêu cầu: thôi theo dõi đọc
            xuLy(kn.ch, p, Arrays.copyOfRange(all, j + 1, j + 1 + can));
        } catch (IOException e) {
            key.cancel();
        }
    }

    static void run(String host, int port) throws Exception {
        Thread.currentThread().setName("vòng-lặp");
        VongLap loop = new VongLap();
        MayChu mc = new MayChu(loop);
        ServerSocketChannel ss = ServerSocketChannel.open();
        ss.bind(new InetSocketAddress(host, port), 128);
        ss.configureBlocking(false);
        ss.register(loop.sel, SelectionKey.OP_ACCEPT);
        Thread cb = new Thread(mc::camBien, "cảm-biến");
        cb.setDaemon(true);
        cb.start();
        Chung.say(Chung.f("[server] lắng nghe %s:%d; vòng lặp sự kiện trên luồng \"vòng-lặp\", nhóm tính %d luồng,", host, port, Chung.NHOM_TINH));
        Chung.say(Chung.f("  nhịp tim %d ms, luồng cảm biến mỗi %d ms, chế độ C giới hạn %d yêu cầu", Chung.NHIP_MS, Chung.CAM_BIEN_MS, Chung.GIOI_HAN));
        Chung.say("Đặc thù Java: vòng lặp tự dựng bằng java.nio Selector (cách Netty làm); hàm chặn chạy trên luồng ảo;");
        Chung.say("  việc CPU dùng supplyAsync(việc, nhómRiêng) – đừng để việc chặn/CPU chiếm ForkJoinPool.commonPool().");
        loop.run(mc::suKienSocket);
    }
}
