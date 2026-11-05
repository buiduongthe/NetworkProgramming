import java.awt.Color;
import java.awt.Dimension;
import java.awt.Graphics2D;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.geom.Line2D;
import java.awt.geom.Rectangle2D;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.DefaultListModel;
import javax.swing.JCheckBox;
import javax.swing.JLabel;
import javax.swing.JList;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.12 – Bảng điều khiển máy chủ ngân hàng (Swing).
 *
 * Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
 * ở GiaoDienChung.CuaSoServerCoSo; tệp này chỉ vẽ khung "Tài nguyên": TỔNG TIỀN HỆ THỐNG (xanh khi khớp, đỏ khi lệch),
 * mỗi tài khoản một thanh số dư (vạch xám = số dư ban đầu), số giao dịch, số vụ khóa chết phát hiện, ba giao dịch mới
 * nhất, tham số đặt lại (số tài khoản, số dư ban đầu, ô chọn "khóa ngây thơ").
 *
 * Chạy:  java -cp out ServerGui [host] [cổng]        (mặc định 127.0.0.1 43762)
 */
public class ServerGui extends GiaoDienChung.CuaSoServerCoSo {
    JLabel nhanTong, nhanTongCt, nhanGd;
    GiaoDienChung.KhungVe cv;
    volatile Map<String, Object> ttVe;
    String veCu;
    DefaultListModel<String> ls;
    JTextField oSoTk, oSoDu;
    JCheckBox oNgayTho;

    ServerGui(String host, int cong) {
        super("Máy chủ ngân hàng MR6.12", "Tài nguyên: các tài khoản",
                bao -> new MayChu.MayChuNganHang("khoa", 2.0, bao, Json.obj()), host, cong, 500, 200, MoPhong.SO_LAN_MAC_DINH);
        xay();
    }

    @Override
    protected void veTaiNguyen(JPanel g) {
        g.add(GiaoDienChung.nhan("TỔNG TIỀN HỆ THỐNG", GiaoDienChung.chu(11, true), new Color(0x404040)));
        nhanTong = GiaoDienChung.nhanNen("–", GiaoDienChung.chu(20, true), new Color(0xe0e0e0), 300, 30);
        g.add(nhanTong);
        nhanTongCt = GiaoDienChung.nhan(" ", GiaoDienChung.chu(11, false), null);
        g.add(nhanTongCt);
        cv = new GiaoDienChung.KhungVe(360, 150, this::veTaiKhoan);
        cv.setBorder(BorderFactory.createLineBorder(new Color(0x808080)));
        g.add(Box.createVerticalStrut(2));
        g.add(cv);
        nhanGd = GiaoDienChung.nhan(" ", GiaoDienChung.chu(12, false), null);
        g.add(Box.createVerticalStrut(2));
        g.add(nhanGd);
        ls = new DefaultListModel<>();
        JList<String> l = new JList<>(ls);
        l.setFont(GiaoDienChung.chuDeu(11, false));
        l.setVisibleRowCount(3);
        l.setFocusable(false);
        JScrollPane c = new JScrollPane(l);
        c.setPreferredSize(new Dimension(360, 58));
        c.setMaximumSize(new Dimension(360, 58));
        c.setAlignmentX(0);
        g.add(Box.createVerticalStrut(2));
        g.add(c);
        JPanel f = new JPanel(new GridBagLayout());
        f.setBackground(GiaoDienChung.NEN);
        f.setAlignmentX(0);
        f.add(GiaoDienChung.nhan("Số tài khoản:"), GiaoDienChung.o(0, 0));
        oSoTk = GiaoDienChung.oNhap(lop.thamSo.get("so_tk"), 4);
        f.add(oSoTk, GiaoDienChung.o(1, 0));
        f.add(GiaoDienChung.nhan("  Số dư ban đầu:"), GiaoDienChung.o(2, 0));
        oSoDu = GiaoDienChung.oNhap(lop.thamSo.get("so_du_dau"), 10);
        f.add(oSoDu, GiaoDienChung.o(3, 0));
        oNgayTho = new JCheckBox("Khóa ngây thơ: nguồn rồi đích (có thể khóa chết)", Chung.so(lop.thamSo.get("khoa_ngay_tho")) != 0);
        oNgayTho.setBackground(GiaoDienChung.NEN);
        GridBagConstraints cc = GiaoDienChung.o(0, 1);
        cc.gridwidth = 4;
        f.add(oNgayTho, cc);
        g.add(Box.createVerticalStrut(4));
        g.add(f);
    }

    @Override
    protected Map<String, Object> thamSoDatLai() {
        try {
            return Json.obj("so_tk", Long.parseLong(oSoTk.getText().strip()), "so_du_dau", Long.parseLong(oSoDu.getText().strip()),
                    "khoa_ngay_tho", oNgayTho.isSelected() ? 1L : 0L);
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("tham số phải là số");
        }
    }

    /** Bấm ô chọn "khóa ngây thơ" (kịch bản demo) – có hiệu lực khi bấm Đặt lại. */
    void datKhoaNgayTho(boolean bat) { if (oNgayTho.isSelected() != bat) oNgayTho.doClick(0); }

    @Override
    protected void capNhatTaiNguyen(Map<String, Object> tt, Map<String, Object> tk) {
        long tong = Chung.so(tt.get("tong")), dau = Chung.so(tt.get("tong_dau")), lech = tong - dau;
        nhanTong.setText(Chung.tien(tong));
        nhanTong.setBackground(lech != 0 ? new Color(0xff6060) : new Color(0x70d070));
        nhanTong.setForeground(lech != 0 ? Color.WHITE : Color.BLACK);
        nhanTongCt.setText("ban đầu " + Chung.tien(dau) + " – " + (lech != 0 ? "LỆCH " + MayChu.tienDau(lech) : "khớp"));
        nhanTongCt.setForeground(lech != 0 ? GiaoDienChung.DO_DAM : GiaoDienChung.XANH_DAM);
        StringBuilder khoa = new StringBuilder();
        for (Object o : Chung.list(tt.get("tai_khoan"))) khoa.append(Chung.map(o).get("ma")).append(Chung.map(o).get("so_du")).append(';');
        if (!khoa.toString().equals(veCu)) {
            veCu = khoa.toString();
            ttVe = tt;
            cv.repaint();
        }
        long kc = Chung.so(tt.get("khoa_chet"));
        nhanGd.setText("Giao dịch: " + tt.get("so_gd") + "   Khóa chết phát hiện: " + kc
                + (Chung.dung(tt.get("khoa_ngay_tho")) ? "   [khóa ngây thơ]" : "   [khóa theo thứ tự]"));
        nhanGd.setForeground(kc > 0 ? GiaoDienChung.DO_DAM : Color.BLACK);
        List<String> moi = new ArrayList<>();
        List<Object> lsGd = Chung.list(tt.get("lich_su"));
        for (int i = 0; i < Math.min(3, lsGd.size()); i++) {
            Map<String, Object> x = Chung.map(lsGd.get(i));
            moi.add(x.get("gio") + " " + x.get("tu") + "→" + x.get("den") + " " + Chung.f("%13s", Chung.tien(Chung.so(x.get("so_tien"))))
                    + " " + x.get("nguoi"));
        }
        List<String> cu = new ArrayList<>();
        for (int i = 0; i < ls.size(); i++) cu.add(ls.get(i));
        if (!cu.equals(moi)) {
            ls.clear();
            for (String x : moi) ls.addElement(x);
        }
    }

    /** Mỗi tài khoản một thanh số dư (≤ 10 dòng mỗi cột, 2 cột khi > 10 tài khoản). */
    void veTaiKhoan(Graphics2D g) {
        g.setColor(Color.WHITE);
        g.fillRect(0, 0, 360, 150);
        Map<String, Object> tt = ttVe;
        if (tt == null) return;
        List<Object> ds = Chung.list(tt.get("tai_khoan"));
        long dau = Chung.so(tt.get("tong_dau")) / Math.max(1, ds.size());
        long lon = 2 * dau;
        for (Object o : ds) lon = Math.max(lon, Chung.so(Chung.map(o).get("so_du")));
        if (lon == 0) lon = 1;
        int cot = ds.size() <= 10 ? 1 : 2, rong = 360 / cot, cao = 15;
        g.setFont(GiaoDienChung.chu(10, false));
        for (int i = 0; i < ds.size(); i++) {
            Map<String, Object> t = Chung.map(ds.get(i));
            double x = (i / 10) * rong + 4, y = (i % 10) * cao + 3;
            long soDu = Chung.so(t.get("so_du"));
            g.setColor(Color.BLACK);
            g.setFont(GiaoDienChung.chu(10, false));
            NganHangGui.chuTrai(g, t.get("ma") + " " + t.get("chu"), x, y + 6);
            double x0 = x + 62, x1 = x + rong - 76;
            g.setColor(new Color(0xeeeeee));
            g.fill(new Rectangle2D.Double(x0, y + 2, x1 - x0, 9));
            g.setColor(soDu >= dau ? new Color(0x4a90d9) : new Color(0xf0a030));
            g.fill(new Rectangle2D.Double(x0, y + 2, Math.max(0, (x1 - x0) * soDu / lon), 9));
            g.setColor(new Color(0x808080));
            double xd = x0 + (x1 - x0) * dau / lon;
            g.draw(new Line2D.Double(xd, y, xd, y + 13));
            g.setColor(Color.BLACK);
            g.setFont(GiaoDienChung.chuDeu(10, false));
            NganHangGui.chuPhai(g, Chung.tien(soDu), x + rong - 6, y + 6);
        }
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int cong = args.length > 1 ? Integer.parseInt(args[1]) : 43762;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, cong).frame.setVisible(true);
        });
    }
}
