import {Button, Code, Group, Loader, Space, Stack, Switch, Text, TextInput, Tooltip} from "@mantine/core";
import {atom, useRecoilState, useSetRecoilState} from "recoil";
import {currentStep, maskUserState, saveToState, topicMetaState} from "../states";
import {fetchMeta} from "../commands";
import debounce from "debounce-promise";
import {useState} from "react";
import {openConfirmModal} from "@mantine/modals";
import {OpenConfirmModal} from "@mantine/modals/lib/context";
import {dialog, fs, path} from "@tauri-apps/api";

const debouncedFetchMeta = debounce(fetchMeta, 500);

const topicUrlState = atom({
  key: "topicUrl",
  default: "",
});
const topicErrorState = atom({
  key: "topicError",
  default: false,
});
const savePathState = atom({
  key: "savePath",
  default: "",
});

const prompts = [
  {desc: "您选择的文件夹不为空", no_subdir: "直接保存", subdir: "存在新建文件夹中"},
  {desc: "您选择的文件夹不为空，而且这个文件夹看起来是一个先前的存档", no_subdir: "更新存档", subdir: "存在新建文件夹中"}
];

const asyncConfirm = (options: OpenConfirmModal) => new Promise<boolean | null>((resolve) => openConfirmModal({
  onConfirm: () => resolve(true),
  onCancel: () => resolve(false),
  onClose: () => resolve(null),
  ...options
}));

export const Config = () => {
  const [topicMeta, setTopicMeta] = useRecoilState(topicMetaState);
  const setSaveTo = useSetRecoilState(saveToState);
  const [maskUser, setMaskUser] = useRecoilState(maskUserState);
  const setStep = useSetRecoilState(currentStep);

  const [topicUrl, setTopicUrl] = useRecoilState(topicUrlState);
  const [topicError, setTopicError] = useRecoilState(topicErrorState);
  const [fetching, setFetching] = useState(false);
  const [savePath, setSavePath] = useRecoilState(savePathState);

  const extractTopic = (topic: string) => {
    const [, name] = topic.match(/https:\/\/shuiyuan.sjtu.edu.cn\/t\/topic\/(\d+)/) || [];
    return name;
  };

  const ready = !topicError && !fetching && topicMeta !== null && savePath !== "";

  const onNextStep = async () => {
    console.log("readDir", await fs.readDir(savePath));
    if (await fs.exists(savePath) && (await fs.readDir(savePath)).length > 0) {
      const filename = `水源_${topicMeta!.title}`;
      const isArchive = (await path.basename(savePath) === filename);

      const prompt = prompts[isArchive ? 1 : 0];
      const create_subdir = await asyncConfirm({
        title: "文件夹不为空",
        children: <Text size="sm">{prompt.desc}</Text>,
        labels: {confirm: prompt.subdir, cancel: prompt.no_subdir},
      });
      if (create_subdir === null) {
        return
      }

      if (create_subdir) {
        const newPath = await path.join(savePath, filename);
        if (await fs.exists(await path.join(savePath, filename))) {
          const confirm = await asyncConfirm({
            title: "存档已存在",
            children: <Text size={"sm"}>在 <Code>{newPath}</Code> 已存在一个存档</Text>,
            labels: {confirm: "更新存档", cancel: "取消"}
          });
          if (!confirm) {
            return;
          }
        }
        setSaveTo(newPath);
      } else {
        setSaveTo(savePath);
      }
    } else {
      setSaveTo(savePath);
    }
    setStep(2);
  }

  return (
    <Stack>
      <TextInput
        label="水源 URL" error={topicError} value={topicUrl}
        styles={{input: {paddingRight: 96}, rightSection: {width: 92}}}
        rightSection={fetching ?
          <Group spacing={"xs"}>
            <Loader size={"xs"}/>
            <Text size={"xs"}>加载中</Text>
          </Group> :
          (topicMeta &&
            <Tooltip label={<Text size={"xs"}>{topicMeta.title}</Text>}>
              <Text size={"xs"} truncate>{topicMeta.title}</Text>
            </Tooltip>
          )}
        onChange={ev => {
          const value = ev.target.value;
          setTopicUrl(value);
          if (value === '') {
            setTopicError(false);
            setTopicMeta(null);
          } else {
            const topic = extractTopic(value);
            if (topic) {
              setTopicError(false);
              (async () => {
                try {
                  setFetching(true);
                  const meta = await debouncedFetchMeta(parseInt(topic));
                  setFetching(false);
                  setTopicMeta(meta);
                } catch (e) {
                  console.log("Topic error");
                  setTopicError(true);
                }
              })()
            } else {
              setTopicError(true);
            }
          }
        }}/>
      <TextInput label="保存到" value={savePath} rightSectionWidth={68} rightSection={
        <Button onClick={() => {
          dialog.open({directory: true}).then(folder => {
            if (folder !== "") {
              setSavePath(folder as string);
            }
          })
        }}>浏览</Button>
      }/>
      <Switch checked={maskUser}
              onChange={ev => setMaskUser(ev.currentTarget.checked)}
              label="打码用户名及头像"
      />
      <Space/>
      <Button disabled={!ready} onClick={() => onNextStep()}>下载</Button>
    </Stack>
  )
}