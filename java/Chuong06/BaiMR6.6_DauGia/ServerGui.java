import java.awt.Color;
import java.awt.GridBagLayout;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import javax.swing.Box;
import javax.swing.DefaultListModel;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.6 – Bảng điều khiển máy chủ đấu giá (Swing).
 *
 * Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
 * ở GiaoDienChung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": món hàng, giá hiện tại, người dẫn đầu, đồng hồ
 * phiên, vài giá gần nhất và tham số đặt lại phiên.
 *
 * Chạy:  java -cp out ServerGui [host] [cổng]        (mặc định 127.0.0.1 43702; phiên mặc định 120 s)
 */
public class ServerGui extends GiaoDienChung.CuaSoServerCoSo {
    JLabel nhanMon, nhanGia, nhanDanDau, nhanDongHo;
    JList<DauGiaGui.DongLs> ls;
    final Map<String, JTextField> oTs = new LinkedHashMap<>();

    ServerGui(String host, int cong) {
        super("Máy chủ đấu giá MR6.6", "Tài nguyên: phiên đấu giá",
                bao -> new MayChu.MayChuDauGia("khoa", 2.0, bao, Json.obj("thoi_gian_s", 120.0)),
                host, cong, 200, 200, MoPhong.SO_LAN_MAC_DINH);
        xay();
    }

    @Override
    protected void veTaiNguyen(JPanel g) {
        JPanel f = GiaoDienChung.hang();
        f.add(new GiaoDienChung.KhungVe(120, 130, gr -> DauGiaGui.veBinhGom(gr, 120, 130, 60, 14, 0.62, true)));
        JPanel t = GiaoDienChung.cot();
        nhanMon = GiaoDienChung.nhan("–", GiaoDienChung.chu(13, true), null);
        nhanGia = GiaoDienChung.nhan("–", GiaoDienChung.chu(24, true), new Color(0x8b0000));
        nhanDanDau = GiaoDienChung.nhan("Dẫn đầu: –");
        nhanDongHo = GiaoDienChung.nhan("Còn lại: –", GiaoDienChung.chuDeu(15, true), null);
        ls = DauGiaGui.danhSach(3, 230, 10);
        for (java.awt.Component c : new java.awt.Component[] {nhanMon, nhanGia, nhanDanDau, nhanDongHo}) t.add(c);
        t.add(Box.createVerticalStrut(2));
        javax.swing.JScrollPane cuon = new javax.swing.JScrollPane(ls);
        cuon.setAlignmentX(0);
        t.add(cuon);
        f.add(Box.createHorizontalStrut(4));
        f.add(t);
        g.add(f);
        JPanel p = new JPanel(new GridBagLayout());
        p.setBackground(GiaoDienChung.NEN);
        p.setAlignmentX(0);
        Object[][] o = {{"gia_khoi_diem", "Giá khởi điểm", 8}, {"buoc_gia", "Bước giá", 8},
                {"thoi_gian_s", "Phiên (s, 0 = ∞)", 5}, {"gia_han_s", "Gia hạn (s)", 5}};
        for (int i = 0; i < o.length; i++) {
            String ma = (String) o[i][0];
            Object v = lop.thamSo.get(ma);
            p.add(GiaoDienChung.nhan(o[i][1] + ":"), GiaoDienChung.o((i % 2) * 2, i / 2));
            JTextField x = GiaoDienChung.oNhap(v instanceof Double d ? Chung.g(d) : v, (int) o[i][2]);
            oTs.put(ma, x);
            p.add(x, GiaoDienChung.o((i % 2) * 2 + 1, i / 2));
        }
        g.add(Box.createVerticalStrut(4));
        g.add(p);
    }

    @Override
    protected Map<String, Object> thamSoDatLai() {
        try {
            return Json.obj("gia_khoi_diem", Long.parseLong(oTs.get("gia_khoi_diem").getText().strip()),
                    "buoc_gia", Long.parseLong(oTs.get("buoc_gia").getText().strip()),
                    "thoi_gian_s", Double.parseDouble(oTs.get("thoi_gian_s").getText().strip()),
                    "gia_han_s", Double.parseDouble(oTs.get("gia_han_s").getText().strip()));
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("tham số phải là số");
        }
    }

    /** Điền ô tham số (kịch bản demo) – như người dùng gõ. */
    void datThamSo(Map<String, Object> ts) { ts.forEach((k, v) -> oTs.get(k).setText(String.valueOf(v))); }

    @Override
    protected void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk) {
        Map<String, Object> mon = Chung.map(tt.get("mon"));
        nhanMon.setText(mon.get("ten") + " (bước " + Chung.tien(Chung.so(mon.get("buoc_gia"))) + ")");
        nhanGia.setText(Chung.tien(Chung.so(tt.get("gia"))));
        Object ai = tt.get("nguoi_dan_dau") == null ? "–" : tt.get("nguoi_dan_dau");
        if (Chung.dung(tt.get("ket_thuc"))) {
            nhanDanDau.setText("THẮNG: " + ai + "  (" + tt.get("so_lan") + " lượt)");
            nhanDanDau.setForeground(GiaoDienChung.DO_DAM);
            nhanDongHo.setText("Kết thúc phiên");
            nhanDongHo.setForeground(new Color(0x606060));
        } else {
            nhanDanDau.setText("Dẫn đầu: " + ai + "  (" + tt.get("so_lan") + " lượt)");
            nhanDanDau.setForeground(Color.BLACK);
            Object con = tt.get("con_lai_ms");
            nhanDongHo.setText(con == null ? "Còn lại: ∞ (không giới hạn)" : "Còn lại: " + DauGiaGui.dongHo(Chung.sod(con) / 1000));
            nhanDongHo.setForeground(con == null || Chung.sod(con) >= 10_000 ? GiaoDienChung.XANH_DAM : GiaoDienChung.DO_DAM);
        }
        List<DauGiaGui.DongLs> moi = new ArrayList<>();
        List<Object> lsu = Chung.list(tt.get("lich_su"));
        for (int i = 0; i < Math.min(3, lsu.size()); i++) {
            Map<String, Object> x = Chung.map(lsu.get(i));
            moi.add(new DauGiaGui.DongLs(Chung.f("%-6s%14s", x.get("nguoi"), Chung.tien(Chung.so(x.get("gia"))))
                    + (Chung.dung(x.get("gia_han")) ? " GH" : ""), false));
        }
        DefaultListModel<DauGiaGui.DongLs> m = (DefaultListModel<DauGiaGui.DongLs>) ls.getModel();
        List<DauGiaGui.DongLs> cu = new ArrayList<>();
        for (int i = 0; i < m.size(); i++) cu.add(m.get(i));
        if (!cu.equals(moi)) {
            m.clear();
            for (DauGiaGui.DongLs x : moi) m.addElement(x);
        }
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43702;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, cong).frame.setVisible(true);
        });
    }
}
