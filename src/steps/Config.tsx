import {Button, Space, Stack, Switch, TextInput} from "@mantine/core";
import {invoke} from "@tauri-apps/api";
import {atom, useRecoilState, useSetRecoilState} from "recoil";
import {currentStep, maskUserState, saveAtState, topicState} from "../states";

const topicUrlState = atom({
    key: "topicUrl",
    default: "",
});

const topicErrorState = atom({
    key: "topicError",
    default: false,
});

export const Config = () => {
    const [topic, setTopic] = useRecoilState(topicState);
    const [saveAt, setSaveAt] = useRecoilState(saveAtState);
    const [maskUser, setMaskUser] = useRecoilState(maskUserState);
    const setStep = useSetRecoilState(currentStep);

    const [topicUrl, setTopicUrl] = useRecoilState(topicUrlState);
    const [topicError, setTopicError] = useRecoilState(topicErrorState);

    const extractTopic = (topic: string) => {
        const [, name] = topic.match(/https:\/\/shuiyuan.sjtu.edu.cn\/t\/topic\/(\d+)/) || [];
        console.log(name);
        return name;
    };

    const validInput = !topicError && topic !== 0 && saveAt !== "";

    return (
        <Stack>
            <TextInput label="水源 URL" error={topicError} value={topicUrl} onChange={ev => {
                const value = ev.target.value;
                setTopicUrl(value);
                if (value === '') {
                    setTopicError(false);
                    setTopic(0);
                } else {
                    const topic = extractTopic(value);
                    if (topic) {
                        setTopicError(false);
                        setTopic(parseInt(topic));
                    } else {
                        setTopicError(true);
                    }
                }
            }}/>
            <TextInput label="保存到" value={saveAt} rightSectionWidth={68} rightSection={
                <Button onClick={() => {
                    invoke("select_folder").then(folder => {
                        if (folder !== "") {
                            setSaveAt(folder as string);
                        }
                    })
                }}>浏览</Button>
            }/>
            <Switch checked={maskUser}
                    onChange={ev => setMaskUser(ev.currentTarget.checked)}
                    label="打码用户名及头像"
            />
            <Space/>
            <Button disabled={!validInput} onClick={() => setStep(2)}>下载</Button>
        </Stack>
    )
}