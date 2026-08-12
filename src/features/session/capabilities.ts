import type {
  SessionCapabilities,
  SessionDetail,
} from "@/features/desktop/types";
import type { RemoteCapabilities } from "@/features/remote/protocol";

export function resolveSessionCapabilities(
  detail: SessionDetail | null,
  isRemote: boolean,
  remoteCapabilities: RemoteCapabilities | null
): SessionCapabilities {
  const hasEditableBlocks =
    detail?.blocks.some((block) => block.editable !== false) ?? false;
  const declared = detail?.capabilities;
  const resume = declared?.resume ?? Boolean(detail?.commands?.resume);
  const fork = declared?.fork ?? Boolean(detail?.commands?.fork);
  const mutationsAllowed = !isRemote || remoteCapabilities?.sessionEdit === true;
  const terminalAllowed = !isRemote || remoteCapabilities?.terminal === true;

  return {
    edit: (declared?.edit ?? hasEditableBlocks) && mutationsAllowed,
    erase: (declared?.erase ?? hasEditableBlocks) && mutationsAllowed,
    restore: (declared?.restore ?? hasEditableBlocks) && mutationsAllowed,
    resume: resume && terminalAllowed,
    fork: fork && terminalAllowed,
    rawTerminal:
      (declared?.rawTerminal ?? (resume || fork)) && terminalAllowed,
    liveStructuredEvents:
      (declared?.liveStructuredEvents ?? false) &&
      (!isRemote || remoteCapabilities?.realtimeEvents === true),
  };
}
