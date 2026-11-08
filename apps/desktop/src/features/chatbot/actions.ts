import { registerActions } from "../../platform/actions";
import { chooseChatbot, toggleChatbot } from "./chatbot";

registerActions({
  toggleChatbot: () => toggleChatbot(),
  chooseChatbot: () => chooseChatbot(false),
});
