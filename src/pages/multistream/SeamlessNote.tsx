import { PageSubbar } from "../../components/AppShell";
import type { useMultistreamPage } from "./model";

export function MultistreamSeamlessNote({
  t,
}: {
  t: ReturnType<typeof useMultistreamPage>["t"];
}) {
  return (
    <section className="page">
      <PageSubbar
        title={t("multistream:title")}
        lede={t("multistream:lede")}
      />
      <p className="muted">{t("multistream:seamlessNote")}</p>
    </section>
  );
}

