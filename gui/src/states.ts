import {atom} from "recoil";
import {Store} from "tauri-plugin-store-api";
import {TopicMeta} from "./bindings";

export const maskUserState = atom({
  key: "maskUserState",
  default: false,
});

export const saveToState = atom({
  key: "saveToState",
  default: "",
});

export const currentStep = atom({
    key: "currentStep",
    default: 0,
});

export const archiveResultState = atom({
    key: "archiveResult",
    default: {
        success: true,
        message: "",
    },
});

const store = new Store("settings");
export const tokenState = atom({
  key: "token",
  default: "",
  effects: [
    ({setSelf, onSet}) => {
      store.get("token")
        .then(token => {
          setSelf(token as string);
        })

      onSet((newValue, _, isReset) => {
                if (isReset || newValue === '') {
                  store.delete("token").then(_ => {
                  });
                } else {
                  store.set("token", newValue).then(_ => {
                  });
                }
            })
        }
    ]
});

let rateLimitInterval: number | undefined = undefined;
export const rateLimitState = atom({
    key: "rateLimit",
    default: 0,
    effects: [
        ({setSelf, onSet}) => {
            onSet((newValue, oldValue, isReset) => {
                console.log("rateLimitState", newValue, oldValue, isReset);
                if (isReset) {
                    setSelf(0);
                    clearInterval(rateLimitInterval)
                    return;
                }
                if (newValue > oldValue) {
                    clearInterval(rateLimitInterval);
                    console.log("rateLimitState", "start interval", newValue);
                    setSelf(newValue);

                    let i = 1;
                    rateLimitInterval = setInterval(() => {
                        console.log("rateLimitState", "interval", newValue - i);
                        setSelf(newValue - i);
                        if (newValue - i <= 0) {
                          clearInterval(rateLimitInterval);
                        } else {
                          i++;
                        }
                    }, 1000);
                }
            });
        }
    ]
})

export const topicMetaState = atom<TopicMeta | null>({
  key: "topicMeta",
  default: null
})