import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.5 – Máy chủ trung chuyển phát trực tiếp 1→n (bản dòng lệnh).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp MR6.2 (âm thanh RTP), MR6.3 (video chia mảnh), Bài 6.1 (server
 * đa luồng), Bài 6.6 (một nguồn – nhiều người nhận). Lõi nằm ở MayChu.java (dùng chung với ServerGui); tệp này chỉ
 * in sự kiện và nhận lệnh bàn phím.
 *
 * Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi server chạy):
 *   trang_thai      bo_dem bat|tat      ngat <tên>      thoat
 *
 * Chạy:  java -cp out ServerPhat [host] [cổng_tcp] [cổng_udp]      (mặc định cổng_udp = cổng_tcp + 1)
 */
public class ServerPhat {
    public static void main(String[] args) throws Exception {
        double t0 = Chung.bayGio();
        Consumer<String> log = Chung.nguoiIn("[server] ");
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int congTcp = args.length > 1 ? Integer.parseInt(args[1]) : 43650;
        int congUdp = args.length > 2 ? Integer.parseInt(args[2]) : congTcp + 1;
        MayChu sv = new MayChu(log);
        sv.batDau(host, congTcp, congUdp);
        CountDownLatch xong = new CountDownLatch(1), daIn = new CountDownLatch(1);
        // Ctrl+C (công cụ demo dừng server bằng SIGINT): vẫn in bảng tổng kết như bản Python.
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            xong.countDown();
            try { daIn.await(4, TimeUnit.SECONDS); } catch (InterruptedException ignored) { }
        }));

        Consumer<String> lenh = dong -> {
            String[] p = dong.split("\\s+");
            log.accept("> " + dong);
            if (p[0].equals("trang_thai")) {
                for (String d : MayChu.bangTrangThai(sv.trangThai())) log.accept(d);
            } else if (p[0].equals("bo_dem") && p.length == 2 && (p[1].equals("bat") || p[1].equals("tat"))) {
                sv.datBoDem(p[1].equals("bat"));
            } else if (p[0].equals("ngat") && p.length == 2) {
                log.accept("Đã ngắt " + sv.ngat(p[1], -1) + " người xem");
            } else if (p[0].equals("thoat")) {
                xong.countDown();
            } else {
                log.accept("lệnh không hợp lệ");
            }
        };
        Chung.chayNen("stdin", () -> Chung.docLenhStdin(t0, lenh, null));
        xong.await();
        for (String d : MayChu.bangTongKet(sv.trangThai())) log.accept(d);
        sv.dung();
        daIn.countDown();
        System.exit(0);
    }
}
