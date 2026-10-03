import 'package:fluent_ui/fluent_ui.dart';

/// Shared surface roles keep the desktop pages and controls visually consistent.
class WorkspacePalette {
  const WorkspacePalette(this.dark);
  final bool dark;
  static WorkspacePalette of(BuildContext context) =>
      WorkspacePalette(FluentTheme.of(context).brightness == Brightness.dark);

  Color get canvas => dark ? const Color(0xff1a1a1a) : const Color(0xffeeede8);
  Color get section => dark ? const Color(0xff272727) : const Color(0xfff8f8f4);
  Color get control => dark ? const Color(0xff343434) : const Color(0xffe5e4dd);
  Color get text => dark ? const Color(0xfff0ead6) : const Color(0xff252521);
  Color get muted => dark ? const Color(0xffb6b3a9) : const Color(0xff737269);
  Color get selected => dark ? const Color(0xff493d32) : const Color(0xffdedbcf);
  Color get accent => text;
}

class Surface extends StatelessWidget {
  const Surface({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(16),
  });
  final Widget child;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) => Card(
    padding: padding,
    backgroundColor: WorkspacePalette.of(context).section,
    borderColor: Colors.transparent,
    borderRadius: BorderRadius.circular(8),
    child: child,
  );
}

FluentThemeData workspaceTheme(Brightness brightness) {
  final colors = WorkspacePalette(brightness == Brightness.dark);
  final base = Typography.fromBrightness(brightness: brightness);
  final shape = WidgetStatePropertyAll<OutlinedBorder>(
    RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
  );
  return FluentThemeData(
    brightness: brightness,
    fontFamily: 'QianbianSans',
    accentColor: colors.accent.toAccentColor(),
    activeColor: colors.dark ? colors.canvas : colors.section,
    inactiveColor: colors.text,
    scaffoldBackgroundColor: colors.canvas,
    acrylicBackgroundColor: colors.canvas,
    micaBackgroundColor: colors.canvas,
    cardColor: colors.section,
    menuColor: colors.control,
    typography: Typography.raw(
      title: base.title!.copyWith(fontSize: 22, height: 1.4, color: colors.text),
      subtitle: base.subtitle!.copyWith(fontSize: 16, color: colors.text),
      body: base.body!.copyWith(fontSize: 14, color: colors.text),
      caption: base.caption!.copyWith(fontSize: 12, color: colors.muted),
    ),
    buttonTheme: ButtonThemeData(
      defaultButtonStyle: ButtonStyle(
        shape: shape,
        padding: const WidgetStatePropertyAll(
          EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        ),
        foregroundColor: WidgetStatePropertyAll(colors.text),
        backgroundColor: WidgetStateProperty.resolveWith((states) =>
            states.contains(WidgetState.hovered) || states.contains(WidgetState.pressed)
                ? colors.control
                : colors.section),
      ),
      filledButtonStyle: ButtonStyle(
        shape: shape,
        padding: const WidgetStatePropertyAll(
          EdgeInsets.symmetric(horizontal: 20, vertical: 12),
        ),
      ),
    ),
    navigationPaneTheme: NavigationPaneThemeData(
      backgroundColor: colors.canvas,
      overlayBackgroundColor: colors.section,
      highlightColor: colors.accent,
      tileColor: WidgetStateProperty.resolveWith((states) =>
          states.contains(WidgetState.selected)
              ? colors.selected
              : states.contains(WidgetState.hovered)
                  ? colors.control
                  : Colors.transparent),
      selectedTextStyle: WidgetStatePropertyAll(TextStyle(
        fontSize: 14, fontWeight: FontWeight.w600, color: colors.text,
      )),
      unselectedTextStyle: WidgetStatePropertyAll(TextStyle(
        fontSize: 14, color: colors.muted,
      )),
      selectedIconColor: WidgetStatePropertyAll(colors.text),
      unselectedIconColor: WidgetStatePropertyAll(colors.muted),
    ),
  );
}
