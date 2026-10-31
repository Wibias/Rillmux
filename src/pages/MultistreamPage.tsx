import { PageSubbar } from "../components/AppShell";
import { InfoIcon } from "../components/FollowedIcons";
import { useMultistreamPage } from "./multistream/model";
import { MultistreamSeamlessNote } from "./multistream/SeamlessNote";
import { MultistreamLayoutControls } from "./multistream/LayoutControls";
import { MultistreamSlotList } from "./multistream/SlotList";
import { MultistreamSearchPanel } from "./multistream/SearchPanel";
import "./MultistreamPage.css";

export function MultistreamPage() {
  const model = useMultistreamPage();
  if (model.kind === "seamless") {
    return <MultistreamSeamlessNote t={model.t} />;
  }
  return (
    <section className="page">
      <PageSubbar
        title={model.t("multistream:title")}
        lede={model.t("multistream:lede")}
        actions={
          model.sessions.length ? (
            <button
              type="button"
              className="button-secondary"
              onClick={() => void model.stopAll()}
            >
              {model.t("multistream:stopAll")}
            </button>
          ) : undefined
        }
      />
      <MultistreamLayoutControls {...model} />
      <MultistreamSlotList {...model} />
      <MultistreamSearchPanel {...model} />
      <p className="ms-footnote">
        <InfoIcon />
        <span>{model.t("multistream:layoutHint")}</span>
      </p>
    </section>
  );
}

