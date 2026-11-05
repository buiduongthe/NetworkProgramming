import java.awt.Color;
import java.awt.Dimension;
import java.awt.GridBagLayout;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.8 – Bảng điều khiển máy chủ đặt vé xem phim (Swing).
 *
 * Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
 * ở GiaoDienChung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": sơ đồ phòng chiếu thu nhỏ (trống / đang giữ / đã
 * bán, ghế nóng viền đỏ), số ghế mỗi loại và tham số đặt lại (thời hạn giữ chỗ, ghế nóng của bộ mô phỏng, giá vé).
 *
 * Chạy:  java -cp out ServerGui [host] [cổng]        (mặc định 127.0.0.1 43722; giữ chỗ 30 s)
 */
public class ServerGui extends GiaoDienChung.CuaSoServerCoSo {
    DatVeGui.SoDoGhe soDo;
    JComponent cv;
    final Map<String, JLabel> nhanDem = new LinkedHashMap<>();
    JLabel nhanNong;
    final Map<String, JTextField> oTs = new LinkedHashMap<>();
    volatile Map<String, Color> mau = Map.of();
    volatile Map<String, Color> vien;
    String veCu;

    ServerGui(String host, int cong) {
        super("Máy chủ đặt vé xem phim MR6.8", "Tài nguyên: phòng chiếu 8 × 12 ghế",
                bao -> new MayChu.MayChuDatVe("khoa", 2.0, bao, Json.obj()), host, cong, 500, 200, MoPhong.SO_LAN_MAC_DINH);
        xay();
    }

    @Override
    protected void veTaiNguyen(JPanel g) {
        JPanel f = GiaoDienChung.hang();
        soDo = new DatVeGui.SoDoGhe(MayChu.HANG, MayChu.COT, 13, 2, 6, 16, new int[] {2, 10}, false);
        cv = new GiaoDienChung.KhungVe(206, 124, gr -> {
            soDo.veNen(gr);
            soDo.ve(gr, mau, vien);
        });
        f.add(cv);
        JPanel t = GiaoDienChung.cot();
        Object[][] loai = {{"trong", "Trống", Color.WHITE}, {"giu", "Đang giữ", DatVeGui.KHAC_GIU}, {"ban", "Đã bán", DatVeGui.DA_BAN}};
        for (Object[] x : loai) {
            JLabel o = new JLabel();
            o.setOpaque(true);
            o.setBackground((Color) x[2]);
            o.setBorder(BorderFactory.createLineBorder(Color.BLACK));
            o.setPreferredSize(new Dimension(16, 16));
            JLabel l = GiaoDienChung.nhan(" " + x[1] + ": –", GiaoDienChung.chu(13, true), null);
            nhanDem.put((String) x[0], l);
            t.add(GiaoDienChung.hang(o, l));
        }
        nhanNong = GiaoDienChung.nhan(" ", GiaoDienChung.chu(11, false), null);
        t.add(Box.createVerticalStrut(4));
        t.add(nhanNong);
        f.add(Box.createHorizontalStrut(6));
        f.add(t);
        g.add(f);
        JPanel p = new JPanel(new GridBagLayout());
        p.setBackground(GiaoDienChung.NEN);
        p.setAlignmentX(0);
        Object[][] o = {{"ttl_s", "Giữ chỗ (s)", 4}, {"ghe_nong", "Ghế nóng", 3}, {"gia_ve", "Giá vé", 6}};
        for (int i = 0; i < o.length; i++) {
            String ma = (String) o[i][0];
            Object v = lop.thamSo.get(ma);
            p.add(GiaoDienChung.nhan(o[i][1] + ":"), GiaoDienChung.o(i * 2, 0));
            JTextField x = GiaoDienChung.oNhap(v instanceof Double d ? Chung.g(d) : v, (int) o[i][2]);
            oTs.put(ma, x);
            p.add(x, GiaoDienChung.o(i * 2 + 1, 0));
        }
        g.add(Box.createVerticalStrut(4));
        g.add(p);
    }

    @Override
    protected Map<String, Object> thamSoDatLai() {
        try {
            return Json.obj("ttl_s", Double.parseDouble(oTs.get("ttl_s").getText().strip()),
                    "ghe_nong", oTs.get("ghe_nong").getText().strip().toUpperCase(),
                    "gia_ve", Long.parseLong(oTs.get("gia_ve").getText().strip()));
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("thời hạn và giá vé phải là số");
        }
    }

    /** Điền ô tham số (kịch bản demo) – như người dùng gõ. */
    void datThamSo(Map<String, Object> ts) { ts.forEach((k, v) -> oTs.get(k).setText(String.valueOf(v))); }

    @Override
    protected void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk) {
        Map<String, Object> dem = Chung.map(tt.get("dem"));
        String[][] ten = {{"trong", "Trống"}, {"giu", "Đang giữ"}, {"ban", "Đã bán"}};
        for (String[] x : ten) nhanDem.get(x[0]).setText(" " + x[1] + ": " + dem.get(x[0]));
        String nong = String.valueOf(tt.get("ghe_nong"));
        Object ai = Chung.map(tt.get("chu")).get(nong);
        nhanNong.setText(nong.isEmpty() ? "Ghế nóng (mô phỏng): ngẫu nhiên"
                : GiaoDienChung.html("Ghế nóng (mô phỏng): " + nong + "\n" + nong + ": " + (ai == null ? "trống" : ai)));
        String khoa = String.valueOf(tt.get("so_do")) + nong;
        if (khoa.equals(veCu)) return;
        veCu = khoa;
        Map<String, Color> m = new HashMap<>();
        String hang = String.valueOf(tt.get("hang"));
        List<Object> sd = Chung.list(tt.get("so_do"));
        for (int r = 0; r < hang.length(); r++) {
            String d = String.valueOf(sd.get(r));
            for (int c = 0; c < d.length(); c++)
                if (d.charAt(c) != '.') m.put("" + hang.charAt(r) + (c + 1), d.charAt(c) == 'g' ? DatVeGui.KHAC_GIU : DatVeGui.DA_BAN);
        }
        mau = m;
        vien = nong.isEmpty() ? null : Map.of(nong, new Color(0xff3030));
        cv.repaint();
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43722;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, cong).frame.setVisible(true);
        });
    }
}
