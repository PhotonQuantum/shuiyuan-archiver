import {atom} from "recoil";
import {invoke} from "@tauri-apps/api";

export const topicState = atom({
    key: "topicState",
    default: 0,
});

export const maskUserState = atom({
    key: "maskUserState",
    default: false,
});

export const saveAtState = atom({
    key: "saveAtState",
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

export const tokenState = atom({
    key: "token",
    default: "",
    effects: [
        ({setSelf, onSet}) => {
            invoke("get_token")
                .then(token => {
                    setSelf(token as string);
                })

            onSet((newValue, _, isReset) => {
                if (isReset || newValue === '') {
                    invoke("del_token")
                } else {
                    invoke("set_token", {token: newValue})
                }
            })
        }
    ]
});