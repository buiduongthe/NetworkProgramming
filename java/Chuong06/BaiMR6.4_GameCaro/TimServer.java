import java.util.List;

/**
 * Bài mở rộng MR6.4 – Tìm server Caro trong LAN bằng UDP broadcast (DISCOVER của Bài 6.5).
 *
 * Chạy: java -cp out TimServer [cổng_khám_phá] [thời_gian_chờ_ms] [địa_chỉ_broadcast]   (mặc định 9999 1000 255.255.255.255)
 */
public class TimServer {
    public static void main(String[] args) {
        int port = Integer.parseInt(args.length > 0 ? args[0] : "9999");
        int waitMs = Integer.parseInt(args.length > 1 ? args[1] : "1000");
        String bcast = args.length > 2 ? args[2] : "255.255.255.255";
        System.out.println("[tim] Phát discover tới " + bcast + ":" + port + ", chờ " + waitMs + " ms");
        List<GiaoThuc.Found> list;
        try {
            list = GiaoThuc.discover(port, waitMs, bcast);
        } catch (Exception e) {
            System.out.println("[tim] Lỗi: " + e.getMessage());
            System.exit(1);
            return;
        }
        System.out.println("[tim] Tìm thấy " + list.size() + " server:");
        for (GiaoThuc.Found f : list)
            System.out.println("[tim]   " + f.name() + " tại " + f.ip() + ":" + f.port() + " – " + f.rooms() + " phòng, " + f.players() + " người");
    }
}
