import {Button, Center, Group, Stack, Text, ThemeIcon} from "@mantine/core";
import {MoodCry, MoodSmile} from "tabler-icons-react";
import {useRecoilState, useRecoilValue, useSetRecoilState} from "recoil";
import {archiveResultState, currentStep, saveToState} from "../states";
import {shell} from "@tauri-apps/api";

export const Finish = () => {
  const [{message, success}, _] = useRecoilState(archiveResultState);
  const saveTo = useRecoilValue(saveToState);
  const setStep = useSetRecoilState(currentStep);
  return (
    <Stack pt={50}>
      <Center>
        <Group>
          <ThemeIcon size={50} radius={50} color={success ? "green" : "orange"}>
            {success ? <MoodSmile size={30}/> : <MoodCry size={30}/>}
          </ThemeIcon>
          <Stack>
            <Text size={"xl"}>{success ? "存档完成" : "存档失败"}</Text>
            {message && <Text>{message}</Text>}
          </Stack>
        </Group>
      </Center>
      <Center>
        <Group>
          <Button variant={success ? "outline" : "filled"}
                  onClick={() => setStep(1)}>{success ? "再存档一个" : "重试"}</Button>
          {success && <Button onClick={() => shell.open(saveTo)}>打开目录</Button>}
        </Group>
      </Center>
    </Stack>
    )
}